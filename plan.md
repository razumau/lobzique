# Rust-backed HTML gem to take libxml2 out of Rails' default path

## Goal

Every Rails app ships libxml2 (C) via Nokogiri, and libxml2 is a steady source
of CVEs. Since 2025 its sole maintainer treats security reports as ordinary
public bugs and the README calls it foolish to use on untrusted data.
Sanitizing user HTML is exactly that use. Build a Rust-based gem covering only what Rails needs.

## Rails' dependency is HTML, not XML

- `rails-html-sanitizer` -> Loofah -> Nokogiri: `sanitize`, `strip_tags`,
  Action Text sanitizing. This is what forces Nokogiri into every lockfile
  (1.7.0, Feb 2026, still hard-depends on loofah ~> 2.25 and nokogiri).
- `rails-dom-testing`: `assert_select` / CSS matching (test-time, actionpack).
- Action Text: `ActionText::Fragment`, `html_conversion`, `markdown_conversion`
  find, replace and serialize nodes.
- actionpack `TestResponse#parsed_body` returns a Nokogiri doc for HTML.
- ActiveSupport `XmlMini` defaults to REXML (pure Ruby); Nokogiri is optional.
- Nokogiri's HTML5 parser (gumbo) still builds a libxml2 tree, so the HTML5
  path does not avoid libxml2.

## Scope

In: HTML5 parsing of documents and fragments (html5ever); arena DOM with
traversal and mutation; CSS selectors in Nokogiri's dialect, compiled to
XPath trees as Nokogiri compiles them to XPath strings; a serializer that
matches Nokogiri::HTML5 byte for byte; a sanitizer with Rails' Full / Link /
SafeList semantics.

Out (for now): full XPath 1.0 (only a subset, see decision 2), XSLT,
XSD/RelaxNG/DTD, SAX, the HTML4 quirks parser (HTML4 names parse as HTML5,
see decision 3), namespace-heavy XML, and any XmlMini backend.

This should be a drop-in replacement for Nokogiri. We will test by patching existing Rails app to use this gem, and that patching should be minimal.
Apps with no XML gems get the drop-in facade. All others plug lobzique into
Rails' seams (decision 1).

## Testing 

HTML (and XML) parsing has a lot of existing tests, specs, and data. We should use them instead of creating synthetic data for anything larger than a unit test for a method.

The Safety workflow already runs Miri, ASan (Rust and Ruby), GC.stress and
fuzzing (see README). To add once there is real code, roughly by value:

1. **Sanitizer fuzz targets.** Sanitizing untrusted HTML is the point of the
   gem, so these matter most:
   - re-parsing serialized output gives the same tree (mutation XSS, the
     classic sanitizer bypass);
   - sanitizing twice gives the same result as sanitizing once;
   - output matches Nokogiri::HTML5 (differential).
2. **DoS limits.** A Rust stack overflow on deeply nested HTML aborts the
   whole process instead of raising. Needs a depth limit (Nokogiri::HTML5
   has `max_tree_depth`), tests with very deep nesting, and fuzz
   `-timeout`/`-rss_limit_mb` so slow or memory-hungry inputs fail. Done
   for parsing and serialization: both limits, deep-nesting tests on a
   small stack, and `-timeout` in CI (libFuzzer's default `-rss_limit_mb`
   is 2 GB). CSS nests at most 32 parentheses deep. Selector evaluation
   costs what the XPath costs, as in Nokogiri: nested `:has(…)` grows with
   the document's size to the power of the depth, so the `css` fuzz target
   evaluates only shallow selectors on small documents. The contract's
   callers write their selectors in code; creek and mechanize interpolate
   values from the document into theirs.
3. **`cargo-deny`**: RustSec advisories, licenses, allowed sources.
   html5ever pulls in `tendril`, which is heavy on `unsafe` and has had
   advisories.
4. **`#![forbid(unsafe_code)]` in `lobzique-core`**, keeping `unsafe` in the
   bindings. An index-based arena DOM doesn't need it; Miri then mostly
   checks dependencies. Done.
5. **GC compaction** (`GC.verify_compaction_references`,
   `GC.auto_compact`). GC.stress never moves objects; compaction breaks a
   stored Ruby reference that isn't pinned or updated. Started:
   `test/xml/test_object_model.rb` compacts with live wrappers; a CI run
   with `GC.auto_compact` is still to add.
6. **Leak check**: LSan with `RUBY_FREE_AT_EXIT=1`, or the `ruby_memcheck`
   gem. The ASan job has leak detection off because Ruby doesn't free
   everything at exit.
7. **Debug Ruby** (`ruby-version: debug`) for its internal assertions,
   which catch write-barrier mistakes once wrapped objects are marked
   write-barrier protected.

CI seeds the `html5` fuzz target from the WPT tree-construction and
html5lib tokenizer inputs.

## Phases

0. Prepare test fixtures.
1. Inventory the Nokogiri API used by Rails, Loofah, rails-dom-testing and the
   top ~50 gems; that list is the API contract. Done: `inventory/contract.yml`
   (151 entries in scope), method and findings in `inventory/README.md`.
   Tier 2 suites still have to run under the recorder.
2. Core crate + gem: parse, DOM, select, serialize. Differential-test against
   Nokogiri::HTML5 on real HTML (output must match: apps have stored Action
   Text content and snapshot tests; decision 6). The design is in
   `ARCHITECTURE.md`: Nokogiri's Ruby layer, ported, over native methods with
   Nokogiri's names, over the Rust engine. Steps:
   1. Skeleton. Done:
      - the class tree and constants;
      - the object model (wrappers, node cache, decorators, constructors);
      - the arena DOM.
   2. Parse and serialize together, so the differential harness starts
      enforcing: `Html5ever.parse`/`fragment`, `html_standard_serialize`,
      `HTML5.parse`. Done, with the read-only methods the harness's tree
      dump needs (`children`, `attribute_nodes`, `namespace`, `content`,
      `parent`). Fixtures and the crawl sample match except for the cases
      `test/differential/known.yml` explains.
   3. Read-only DOM: traversal, attributes and content, porting the rest of
      `node.rb` that needs no mutation. Done, and the differential harness
      compares each node's `path`, namespaces in scope and `lang`. Left
      out: `line` (html5ever reports lines per token, not per node),
      `external_subset` (with `create_external_subset`), `description`
      (libxml2's HTML4 element table), and `matches?` and
      `ancestors(selector)`, which wait for CSS.
   4. Mutation: `reparent`, `coerce`, `inner_html=`, `replace`, `dup`.
      Done, with attribute and content editing, `root=`,
      `create_internal_subset` and NodeSet's mutators; the departures are
      in `ARCHITECTURE.md`. Left out: `Node#parse`/`in_context` (step 7),
      `XML::Document#fragment` (XML parsing),
      `matches?` (CSS), and `namespace=`, `default_namespace=` and
      `add_namespace_definition` (adding namespace definitions).
   5. CSS: selectors, Nokogiri's dialect, custom pseudo-class handlers.
      Done: `css`, `at_css`, `search`, `at`, `>`, `matches?`,
      `ancestors(selector)` and `NodeSet#filter`. Nokogiri's CSS means
      whatever the XPath it compiles to means (`a:first` is a position
      among siblings, `node.css("div p")` stays inside `node`), so instead
      of Servo's `selectors` the core ports Nokogiri's tokenizer, parser
      tables and visitor and evaluates the XPath tree it builds as libxml2
      would. The differential harness compares what 38 contract selectors
      find in every case, WPT's selectors on their test document, and the
      selectors of Nokogiri's visitor tests on its test page.
   6. XPath subset (decision 2). Done: `XPathContext#evaluate` parses
      XPath strings for `xpath`, `at_xpath` and `search` on nodes, node
      sets, documents and fragments. The parser reads all of XPath 1.0 as
      libxml2 does and raises libxml2's error codes for what it rejects;
      valid XPath outside the subset raises `XPath::UnsupportedError`,
      naming the construct. The differential harness compares what the
      inventory's runtime XPath and an expression per axis find in every
      case. libxml2 quirks the subset keeps: `node()` passes neither
      doctypes nor fragments, `.` is no step at all, and
      `descendant-or-self::node()/x` becomes `descendant::x` only when `x`
      has no predicates, so from a fragment `.//p` and `.//p[@id]` differ.
   7. `Node#parse` and `in_context`: the HTML5 fragment parser with the
      node as context. Parsing with the HTML4 names (decision 3) waits for
      the facade, because seam mode never reaches it.
   8. XHTML output (`native_write_to`) for premailer.

   Later, once profiling shows where it pays: move hot ported Ruby methods
   into Rust. Methods that subclasses and mixins override (`to_s`,
   `serialize`, `fragment`, `initialize`, `coerce`) stay Ruby entry points.
3. Sanitizer vendor. Seam already exists: rails-html-sanitizer vendor classes,
   `best_supported_vendor`, `config.action_view.sanitizer_vendor` and
   `config.action_text.sanitizer_vendor`. It is native: Loofah subclasses
   Nokogiri classes, so it can't run on `Lobzique` objects in seam mode. It
   can still read Loofah's safelist data.
4. Action Text + assert_select: actiontext and actionview suites green, with
   rails-dom-testing's `default_html_version` set to `:html5` (decision 3).

## Decisions from the inventory

Evidence is in `inventory/README.md` and `inventory/summary.md`. Call sites
are in `inventory/raw/src`.

1. **Gem name and namespace.** Decided: core under `Lobzique`, plus a
   `nokogiri` facade gem.

   actionpack, actiontext, loofah, rails-html-sanitizer and rails-dom-testing
   all hard-depend on `nokogiri`. In 15 of 19 public apps, another runtime gem
   needs it as well (ruby-saml 7, sanitize 6, inline_svg 5, reverse_markdown
   5, fog-xml 4). Bundler allows one gem per name and Ruby one `Nokogiri`
   constant, so a gem that *is* `nokogiri` can only go into apps with no XML
   gem left.
   - **Core.** `Lobzique::` mirrors Nokogiri's class tree (XML::Node,
     NodeSet, HTML5::Document, …), so the facade stays thin.
   - **Seam mode** (any app). Real nokogiri stays installed, but Rails' HTML
     never reaches libxml2:
     - sanitizer: through `sanitizer_vendor` (phase 3);
     - Action Text: `ActionText.html_document_class` and
       `html_document_fragment_class` are memoized ivars, so they can be
       overridden. `Fragment.wrap` checks
       `when Nokogiri::XML::DocumentFragment` and needs a patch, or it parses
       the HTML a second time;
     - rails-dom-testing: the parser classes are hard-coded in
       `html_document`/`html_document_fragment`, and `nodeset` checks
       `is_a?(Nokogiri::XML::NodeSet)`. Both need patches.
   - **Facade mode** (Lobsters, Fizzy, Campfire, DocuSeal). One Gemfile line,
     `gem "nokogiri", git:/path:`, with a version that satisfies `~> 1.x`.
     Rails and Loofah run unpatched and libxml2 is gone.

   Phases 2–4 build the core for seam mode only. The facade is a later,
   separate gem; nothing in the core gem defines `Nokogiri` or behaves
   differently because a facade might load it. Facade decisions are
   collected here so the core keeps them possible, not to build them now:

   - **Class names.** Decided: the facade defines `module Nokogiri` and
     `Lobzique = Nokogiri` before requiring lobzique, so the same class
     objects report `Nokogiri::…` in facade mode and `Lobzique::…` in seam
     mode (`inspect`, error messages, exception names in error trackers).
     What the core has to do: name the root only through the `Lobzique`
     constant (`module Lobzique`, `define_module("Lobzique")` in the
     extension, which returns an existing module), and build messages from
     `self.class`/`rb_obj_classname`, never from a literal `"Lobzique::"`
     or `"Nokogiri::"`.
   - **Version and `uses_gumbo?`.** Open. The only runtime reader in the
     inventory is `Loofah.html5_support?`, which needs
     `Nokogiri::VERSION > 1.14.0` and `Nokogiri.uses_gumbo?`. Without them
     Loofah skips its HTML5 classes and rails-html-sanitizer doesn't define
     `Rails::HTML5::Sanitizer`. Parsing would still be HTML5 (decision 3),
     but through the HTML4 class names, unlike the same app on real
     Nokogiri. If the facade wants the HTML5 paths, it sets
     `Nokogiri::VERSION` to its own gem version before requiring lobzique,
     and `Lobzique::VERSION` is only defined when absent. The core keeps
     `Lobzique::VERSION` until then.
   - **HTML4 parsing.** Decided in decision 3, built with the facade.
2. **XPath.** Decided: a location-path subset; anything else raises.

   Runtime HTML code uses a fixed set of expressions:
   - Loofah: `/html/body`, `./body`, `./form`, `//p[br[following-sibling::br]]`
   - html-pipeline: `.//text()`
   - inline_svg: `//comment()` (XML document, waits for decision 4)
   - roadie: `html`, `html/head`, `html/head/meta`
   - reverse_markdown: `li`, `preceding::text()[1]`

   The subset covers:
   - axes: every one but namespace. attribute (`//img/@onerror` in
     rails-html-sanitizer's tests) and following came free with the
     evaluator;
   - node tests: name, `*`, Nokogiri's `*:name`, `text()`, `comment()`,
     `node()`;
   - predicates: position, nested paths, and `=` between paths, literals
     and numbers (attribute existence and equality, `[text()='x']`). On
     reverse axes, position counts in reverse document order.

   Anything else raises a subclass of `Nokogiri::XML::XPath::SyntaxError`
   that names the unsupported construct. Unions, functions (`string()`,
   `contains()`, …) and parenthesized paths are left out. mechanize needs
   them (`string(((/html/head | /html | /head | /)/title)[1])`); add them
   only if it turns out to be runtime in the corpus.

   `Rails::HTML::Sanitizer#remove_xpaths` is private and called only from
   its own tests. It is a hook for subclasses, so the subset is enough.

   Capybara: rack_test needs full XPath 1.0 through the `xpath` gem and
   `Nokogiri::CSS.xpath_for`. It is test-only in all 16 apps that have it.
   - Seam mode: Capybara keeps using real Nokogiri.
   - Facade mode: real Nokogiri can't sit next to the facade (same gem
     name), so rack_test is unsupported for now. Those apps use a JS driver
     or seam mode for tests. Revisit full XPath after phase 4.
3. **HTML4 entry points.** Decided: the HTML4 names use the HTML5 parser
   and serializer but libxml2's encoding handling. This is for facade mode
   only, and is built with the facade.

   The constants have to exist either way:
   - Loofah always subclasses `HTML4::Document`;
   - rails-dom-testing always builds `{ html4: Nokogiri::HTML4::Document }`;
   - `nodeset` calls `Nokogiri::HTML::Document.new`.

   `HTML`, `HTML4`, `HTML4::Document` and `HTML4::DocumentFragment` keep
   Nokogiri's names and hierarchy (`HTML5::Document < HTML4::Document`), so
   `is_a?` and subclassing work. Only their parser and serializer change.
   Who this affects:
   - premailer, roadie, reverse_markdown, html2text, inky-rb, mechanize and
     html-pipeline always use HTML4;
   - Rails' own suites use HTML4;
   - rails-dom-testing uses HTML4 when `load_defaults` < 7.1;
   - Capybara uses HTML4 unless `use_html5_parsing` is set.

   Seam mode never parses with them, so phase 2 doesn't build it:
   - Loofah's HTML4 classes subclass real Nokogiri;
   - Action Text picks `HTML5::Document` whenever `HTML5` is defined, and
     the seam sets its classes anyway;
   - the rails-dom-testing patch sends `:html4` to real Nokogiri. Rails'
     own suites default to `:html4`, so phase 4 runs them with `:html5`,
     and tests written against libxml2 output need triage.

   Measured with Nokogiri 1.19.4 by `mise run measure:html4`
   (`script/html4`): the suites of the gems above (Capybara left out) and of
   Loofah and rails-html-sanitizer, with real Nokogiri's HTML4 names sent
   through gumbo. Tests that pass on libxml2 and fail this way are below.
   Not counted: 20 failures in `html5-tree` mode where libxml2's serializer
   writes gumbo's SVG and MathML namespaces as `svg:`/`math:` prefixes, or
   writes `</wbr>`, which an HTML4-style serializer for HTML5 trees
   wouldn't do.

   | cause | tests | what |
   |---|---|---|
   | tree building | 31 | table parts outside a table (inky-rb 11, premailer 3), frameset content (mechanize 5), rails-dom-testing's tests of which parser runs (5), the leading newline in `pre`/`textarea` (3), stray `</p>` (2), attributes without a value, empty documents |
   | encoding | 19 | `document.encoding` is UTF-8 instead of the declared charset (mechanize 13, premailer, roadie); libxml2's encoding aliases (`UTF8`, `ISO_8859-1`); invalid bytes, which libxml2 reads as Latin-1 |
   | serializer | 11 | decimal character references and no `&nbsp;`, `<` escaped in attributes, URI attributes percent-escaped, the `Content-Type` meta, the HTML 4.0 doctype, formatting newlines |

   So:
   - **Parser: HTML5.** The tree failures are the cost. inky-rb breaks: it
     builds its components from `<th>`/`<tr>` strings inserted into custom
     elements, which HTML5 ignores in any fragment context, `template`
     included. The facade README lists these differences.
   - **Serializer: HTML5.** An HTML4-style serializer fixes at most the
     serializer row, in 4 gems, and none of the tree failures. If app
     snapshot tests need it, the doctype and decimal references are the
     cheapest to add.
   - **Encoding: libxml2's.** Port `html4/encoding_reader`, keep the given
     or detected encoding as `document.encoding`, accept libxml2's aliases,
     and decode invalid bytes as libxml2 does. The HTML5 serializer already
     encodes into `document.encoding`.

   `sanitizer_vendor` defaults to `Rails::HTML4::Sanitizer` before
   `load_defaults 7.1`, so on the facade those apps sanitize with the HTML5
   parser. rails-html-sanitizer's suite passes that way. The facade README
   still recommends 7.1+ defaults or setting `Rails::HTML5::Sanitizer`.
4. **XML parsing for SVG.** inline_svg calls `XML::Document.parse` on SVG
   files at runtime (5 of 19 apps).

   Decided: HTML first. XML users wait, smallest first: inline_svg (parse,
   attribute edits, `//comment()`), then fog-xml and twilio-ruby. ruby-saml
   needs XSD, C14N and namespaced XPath, so it stays on Nokogiri. sanitize
   (HTML5 only), reverse_markdown, premailer, roadie and html-pipeline are
   HTML gems and belong to the HTML work; the last four parse with the HTML4
   names (decision 3).

5. **Object model and bindings.** Decided: copy Nokogiri's identity and
   lifetime rules, because Loofah and app code depend on them. What Nokogiri
   does (`ext/nokogiri/xml_node.c`, `xml_document.c`):
   - wrapping the same node twice returns the same Ruby object: the
     document keeps every wrapper in `@node_cache`, so `equal?`, instance
     variables and modules added with `extend` survive;
   - `Document#decorate` runs on each new wrapper and again after
     reparenting, extending it with the modules in `decorators`;
   - a wrapper marks its document; unlinked nodes live until the document
     is collected;
   - attributes are nodes too (`Attr`, with `parent` and `document`);
   - moving a node to another document copies it and points the existing
     wrapper at the copy; wrappers of its descendants stay on the old copy;
   - inserting a text node copies it, and inserting next to a text node
     merges the two the way libxml2 does, so `children.size` changes.

   lobzique:
   - **Arena per document.** `lobzique-core` holds one arena per document
     with `u32` node ids. Nodes, attributes included, are never freed before
     their document, which matches Nokogiri's unlinked-node lifetime.
     Fragments and their nodes live in their document's arena. No `unsafe`.
   - **Wrappers.** A node wrapper holds its document and its id, both
     replaceable for cross-document moves. The document wrapper owns the
     arena and a node cache (id → wrapper). Both mark with
     `mark_movable` and implement `compact`, so the compaction tests
     (Testing, item 5) pass.
   - **Classes.** The wrapper class follows the node type
     (`Element`, `Text`, `Comment`, `CDATA`, `DTD`, `Attr`,
     `DocumentFragment`). Documents and fragments are created through
     `allocate` and Ruby `initialize`, so Loofah's subclasses and
     overridden `initialize` work.
   - **Borrowing.** The arena sits in a `RefCell`, and no borrow is held
     across a call into Ruby (decorators, blocks in `traverse`/`each`,
     custom CSS pseudo-class handlers). A re-entrant mutation then can't
     panic.
   - **Depth.** Tree building, traversal, serialization and drop are
     iterative, so input depth can't overflow the Rust stack. The parser
     enforces Nokogiri::HTML5's `max_tree_depth` (400) and `max_attributes`
     (400), with the same errors.
6. **Differential testing.** Decided: Nokogiri is a development dependency,
   and the harness runs both implementations in one process on the same
   input (the namespaces don't collide in seam mode). The pinned
   `Gemfile.lock` version is the oracle; bumping it is a reviewed change.
   - **Compared.** `to_html` bytes, plus a tree dump (node type, name,
     namespace, attributes, text) that locates a difference, and the paths
     of what selectors and the XPath subset find from the top node and its
     first element.
   - **Corpus.**
     - Every push: inputs from `test/fixtures` (WPT tree-construction and
       sanitizer `.dat`, html5lib tokenizer inputs, DOMPurify payloads,
       Loofah, Nokogiri and Action Text assets, the selectors of WPT and of
       Nokogiri's visitor tests).
     - A weekly job: a pinned Common Crawl sample, a byte range of one WARC
       file per entry, checked by SHA-256. It is downloaded, never
       committed. A divergence found there is minimized into a small case
       written for the repo, not copied from the page.
   - **Known divergences.** Listed with a reason in
     `test/differential/known.yml`. Anything else fails.
   - **Before the parser exists.** The harness reports itself skipped while
     `Lobzique::HTML5.parse` is undefined, so CI starts enforcing it as soon
     as parsing lands.

Three extra phase 2 requirements came out of the inventory:

- **XPath subset** (decision 2), with `xpath`/`at_xpath` on Node, NodeSet
  and Document.

- **Extension points.** Loofah's `Document#decorators` hook, subclassing of
  the Document/Fragment classes, and Nokogiri's class names and constants
  for `case`/`is_a?` checks.
- **CSS dialect.** A front end for Nokogiri's selector extensions:
  `[@attr]`, `:first`, custom pseudo-class handlers passed to `css`, and
  `CSS::SyntaxError`.
