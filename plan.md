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
traversal and mutation; CSS selectors (Servo `selectors`); a serializer that
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

## Phases

0. Prepare test fixtures.
1. Inventory the Nokogiri API used by Rails, Loofah, rails-dom-testing and the
   top ~50 gems; that list is the API contract. Done: `inventory/contract.yml`
   (151 entries in scope), method and findings in `inventory/README.md`.
   Tier 2 suites still have to run under the recorder.
2. Core crate + gem: parse, DOM, select, serialize. Differential-test against
   Nokogiri::HTML5 on real HTML (output must match: apps have stored Action
   Text content and snapshot tests).
3. Sanitizer vendor. Seam already exists: rails-html-sanitizer vendor classes,
   `best_supported_vendor`, `config.action_view.sanitizer_vendor` and
   `config.action_text.sanitizer_vendor`. It is native: Loofah subclasses
   Nokogiri classes, so it can't run on `Lobzique` objects in seam mode. It
   can still read Loofah's safelist data.
4. Action Text + assert_select: actiontext and actionview suites green.

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
     Rails and Loofah run unpatched and libxml2 is gone. The facade stubs
     `VERSION` (> 1.14.0) and `uses_gumbo?` (true), so Loofah,
     rails-html-sanitizer, Action Text and rails-dom-testing all pick their
     HTML5 paths.
   - Still open for phase 2: are the `Nokogiri::*` and `Lobzique::*`
     constants the same class objects, and which name does `Class#name`
     report? The answer shows up in `inspect`, error messages and snapshot
     tests.
2. **XPath.** Decided: a location-path subset; anything else raises.

   Runtime HTML code uses a fixed set of expressions:
   - Loofah: `/html/body`, `./body`, `./form`, `//p[br[following-sibling::br]]`
   - html-pipeline: `.//text()`
   - inline_svg: `//comment()` (XML document, waits for decision 4)
   - roadie: `html`, `html/head`, `html/head/meta`
   - reverse_markdown: `li`, `preceding::text()[1]`

   The subset covers:
   - axes: child, descendant(-or-self), self, parent, ancestor(-or-self),
     following-sibling, preceding-sibling, preceding;
   - node tests: name, `*`, `text()`, `comment()`, `node()`;
   - predicates: attribute existence, attribute equality, position, and
     nested paths. On reverse axes, position counts in reverse document
     order.

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
3. **HTML4 entry points.** Decided: the HTML4 names parse and serialize
   with the HTML5 parser (facade mode only; seam mode keeps real Nokogiri).

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

   `sanitizer_vendor` defaults to `Rails::HTML4::Sanitizer` before
   `load_defaults 7.1`, so on the facade those apps' `sanitize` output
   changes. The facade README tells them to use 7.1+ defaults or set
   `Rails::HTML5::Sanitizer` explicitly.

   Document the libxml2 HTML4 vs HTML5 differences:
   - tree building: adoption agency, foster parenting, implied end tags,
     fragment context;
   - serialization: libxml2 percent-escapes URI attributes and encodes
     non-ASCII by document encoding.

   Expect Rails suite tests written against libxml2 output to need triage
   in phase 4.
4. **XML parsing for SVG.** inline_svg calls `XML::Document.parse` on SVG
   files at runtime (5 of 19 apps).

   Decided: HTML first. XML users wait, smallest first: inline_svg (parse,
   attribute edits, `//comment()`), then fog-xml and twilio-ruby. ruby-saml
   needs XSD, C14N and namespaced XPath, so it stays on Nokogiri. sanitize
   (HTML5 only), reverse_markdown, premailer, roadie and html-pipeline are
   HTML gems and belong to the HTML work; the last four parse with the HTML4
   names (decision 3).

Three extra phase 2 requirements came out of the inventory:

- **XPath subset** (decision 2), with `xpath`/`at_xpath` on Node, NodeSet
  and Document.

- **Extension points.** Loofah's `Document#decorators` hook, subclassing of
  the Document/Fragment classes, and Nokogiri's class names and constants
  for `case`/`is_a?` checks.
- **CSS dialect.** A front end for Nokogiri's selector extensions:
  `[@attr]`, `:first`, custom pseudo-class handlers passed to `css`, and
  `CSS::SyntaxError`.
