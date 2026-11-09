# Architecture

Nokogiri is a Ruby library on top of a small native core: its C extension
wraps libxml2 and gumbo, and everything users and gems build on (coercing
strings into nodes, fragment parsing rules, serialization options, the
decorator hooks Loofah uses) is Ruby. lobzique keeps that shape and replaces
only the native core. The Ruby layer is Nokogiri's own code, ported file by
file. The native methods it calls have Nokogiri's names and signatures. Under
them is a Rust engine instead of libxml2.

This is a rule, not a starting point. Nothing is designed as lobzique's own
API with Nokogiri's names aliased onto it. When Nokogiri's code and ours
differ, the difference is listed below, with the reason.

```
lib/lobzique/**            Nokogiri's Ruby layer        ← lib/nokogiri/**
ext/lobzique/src/**        Nokogiri's native methods    ← ext/nokogiri/*.c
crates/lobzique-core       the engine                   ← libxml2 + gumbo
```

`lib/lobzique/xml/node.rb` and `ext/lobzique/src/xml/node.rs` are the Ruby
and native halves of `XML::Node`. Every class follows the same path scheme.

## Ruby layer

**Port rule.**
- Each file in `lib/lobzique/` adapts the Nokogiri file of the same path and
  says so in its header (`LICENSE-NOKOGIRI.md` has Nokogiri's MIT notice).
- A method of an in-scope class comes over once the native methods it calls
  exist. That covers every such method, not just the inventory's contract
  entries, because gems reach Nokogiri methods through each other.
- A method is left out only when it needs something out of scope: XML
  parsing, adding namespace definitions, XInclude, C14N, SAX, XSLT.
- JRuby branches are dropped.

**Departures are marked.** Every change to ported code has a comment starting
with `# lobzique:`. `grep -rn "lobzique:" lib` lists everything to recheck
when the Nokogiri version in `Gemfile.lock` moves.

**Not ported to Ruby.**
- `css/parser`, `tokenizer`, `xpath_visitor` and `css/node`: ported to the
  core instead (`css`), so `Searchable` gets a compiled `CSS::Selector`
  where Nokogiri gets an XPath string.
- SAX, XSLT, Builder, Reader, Schema and RelaxNG.

### HTML4 names (`lib/lobzique/html4`)

The HTML4 names parse with the HTML5 parser but decode their input as
libxml2 2.13 does (plan.md decision 3), so `document.encoding` and the
text are what Nokogiri gives. `HTML4::Document.parse` is Nokogiri's;
`read_memory` and `read_io`, native in Nokogiri, are Ruby here and call
`EncodingReader.decode`, which follows libxml2's source:

- a given encoding (or a Ruby string's tag, or what
  `EncodingReader.detect_encoding` finds) decodes the input; a built-in
  libxml2 decoder's name becomes `document.encoding` canonicalized,
  any other as written; an unsupported one is a warning, and the guess
  follows;
- with none: a BOM; UTF-8 for input starting `<?xm`; the first supported
  `<meta>` declaration before any non-ASCII byte, with the last one
  declared as `document.encoding` (`htmlCheckMeta`); else at the first
  non-ASCII byte a raw search for `HTTP-EQUIV`…`CHARSET=`, within what an
  IO has buffered (`htmlFindEncoding`); else ISO-8859-1.

`detect_encoding` and the `<meta>` rule read `<meta>` elements from an
extra HTML5 parse of the input as ISO-8859-1, where Nokogiri runs libxml2's
SAX parser. Only binary strings and IO without an encoding get there, and
only input containing `<meta`. `script/differential --mode html4` compares
`encoding` and the decoded text with Nokogiri on the fixtures and the crawl.

Fragments parse with the HTML5 fragment parser, in `body` or in the context
given; `HTML4::Document#fragment` passes none, where Nokogiri passes the
root, whose HTML5 context would wrap the input in a `body`.

**Facade-safe.**
- Code inside `module Lobzique` refers to constants relatively or through
  `Lobzique`.
- Messages are built from classes (`"must be a #{XML::Node}"`), never from a
  literal root name.
- So a facade that sets `Lobzique = Nokogiri` gets `Nokogiri::…` in every
  message (plan.md decision 1).

### Sanitizer vendor (`lib/lobzique/sanitizer`)

`require "lobzique/sanitizer"`, not loaded by `require "lobzique"`
(plan.md phase 3). It ports from Loofah only what names Nokogiri's classes:
- `Document` and `DocumentFragment`, subclasses of lobzique's HTML5 ones,
  with Loofah's decorators, fragment parsing and serialization
  (`concerns.rb`);
- `scrub!` for nodes, node sets, fragments and documents, whose `case`
  names lobzique's classes;
- the `:escape` and `:newline_block_elements` scrubbers, which build text
  nodes with `Nokogiri::XML::Text.new`.

Everything else is Loofah's and rails-html-sanitizer's code, unchanged:
the scrubbers, `Loofah::HTML5::Scrub` and the sanitizer concerns, which
`FullSanitizer`, `LinkSanitizer` and `SafeListSanitizer` include with
lobzique's parser in place of Loofah's. Ported files name their Loofah
source, and changes carry `# lobzique:` comments, as in the rest of `lib/`.

One addition of lobzique's own: `scrub!` escapes the text of raw-text
elements (`style`, `xmp`, …) inside a `select` (`EscapeRawTextInSelect`). html5ever keeps them there, following the
standard's customizable select; browsers on the older rules ignore their
tags and would parse the text as markup.

### Action Text seam (`lib/lobzique/action_text.rb`)

`require "lobzique/action_text"`, not loaded by `require "lobzique"`
(plan.md phase 4). It sets the instance variables that memoize
`ActionText.html_document_class` and `html_document_fragment_class` to
lobzique's HTML5 classes, and prepends a `Fragment.wrap` that takes a
lobzique fragment as it is; Action Text's own checks for
`Nokogiri::XML::DocumentFragment` and would parse the fragment's HTML
again. The rest of Action Text runs unchanged on lobzique nodes.

### rails-dom-testing seam (`lib/lobzique/rails_dom_testing.rb`)

`require "lobzique/rails_dom_testing"`, not loaded by `require "lobzique"`
(plan.md phase 4). Three prepended methods:
- `Rails::Dom::Testing.html_document` and `html_document_fragment` give
  lobzique's HTML5 classes where rails-dom-testing's own choice is
  Nokogiri's, so its version checks and errors are its own and `:html4`
  stays on Nokogiri (decision 3);
- `SelectorAssertions#nodeset` wraps a lobzique node in lobzique's
  `NodeSet`;
- `HTMLSelector#filter`, ported from rails-dom-testing 2.3.0, builds
  lobzique's `NodeSet` for lobzique matches. It is private and changed in
  2.3.0, so the seam accepts only 2.3.x.

Nokogiri objects take rails-dom-testing's code in all three.

### The nokogiri facade (`facade/`)

A separate gem named `nokogiri` (plan.md phase 5), never pushed to
RubyGems. `facade/lib/nokogiri.rb` defines `module Nokogiri`, sets
`Lobzique = Nokogiri` and then requires lobzique, which reopens that module,
so every class reports `Nokogiri::…`. What Nokogiri's extension defines and
lobzique's doesn't, the libxml2 constants that `version/info.rb` reads, is in
`facade/lib/nokogiri/extension.rb`. Ported files follow the rest of `lib/`:
Nokogiri's path, a header naming it, `# lobzique:` on changes.

One patch to Loofah, which otherwise runs unchanged:
`facade/lib/nokogiri/loofah.rb` prepends to `Loofah::ScrubBehavior::Node` the
`EscapeRawTextInSelect` step that `Lobzique::Sanitizer`'s `scrub!` ends with,
in the outermost `scrub!` only, since escaping twice would double-escape.
Loofah requires nokogiri before it defines that module, so the patch waits
for it with `const_added` hooks.

Its tests run in their own bundle (`test/facade/Gemfile`, `rake
test:facade`), with no real Nokogiri to compare against: the facade's own,
and Loofah's, rails-html-sanitizer's and rails-dom-testing's suites from
`test/fixtures` as they are, with what's left out listed with a reason.

## Native boundary (`ext/lobzique`)

The extension defines the classes and methods that Nokogiri's C extension
defines, with the same names, arity and visibility. The ported Ruby then
calls `add_child_node`, `native_content=`, `html_standard_serialize` or
`DocumentFragment.native_new` exactly as Nokogiri's does. Each native method
converts arguments, makes one call into the core and wraps the result. Logic
that isn't conversion belongs in the core.

`lib.rs` defines the module tree and the native classes in the order of
Nokogiri's `Init_nokogiri`. The Ruby files then reopen them. Pure-Ruby
classes, such as the syntax errors, `SaveOptions` and `NodeSet`, are defined
only in Ruby; their native files (`xml/syntax_error.rs`, `xml/node_set.rs`)
only build instances.

### Node objects

Every node object, documents included, wraps the same Rust struct
(`xml/node.rs`):

```rust
struct Node {
    document: Cell<Option<Opaque<Value>>>,   // the Ruby document object
    id: Cell<Option<NodeId>>,                // index in that document's arena
    arena: OnceCell<RefCell<dom::Document>>, // set only on the document itself
}
```

A node is its document plus an id. A document is the node that owns the
arena, which is how `Document < Node` works with a single data type.
`document` and `id` are cells because a node moved into another document is
repointed, as in Nokogiri.

What Nokogiri guarantees, and how lobzique keeps it:

- **One object per node.**
  - The document's `@node_cache` (Nokogiri's ivar name) is a Ruby Array
    indexed by node id.
  - `wrap` returns the cached object, or creates one whose class follows the
    node type, using Nokogiri's table.
  - So `equal?`, Hash keys, instance variables and `extend`ed modules all
    survive.
  - Ruby marks and compacts the cache itself, so no GC code is needed for it.
- **Decorators.** Every new node object goes through `Document#decorate`, so
  Loofah's `decorators(XML::Node) << ScrubBehavior::Node` applies everywhere.
  `wrap` never calls `initialize`.
- **Constructors behave as in Nokogiri:**

  | call | what runs |
  |---|---|
  | `Node.new`, `Text.new`, `Comment.new`, `CDATA.new` | native: create, wrap (decorates), `initialize(*args, &block)`, then yield |
  | `Document.new(*args)` | native: new arena, `@node_cache`/`@decorators`/`@errors`, then `initialize(*args)` |
  | `HTML4::Document.new` | same, starting with the HTML 4.0 doctype libxml2's `htmlNewDoc` adds; `HTML5::Document.new` inherits it |
  | `DocumentFragment.new(doc, …)` | Ruby: `native_new(doc)` (wrapped, decorated), then `initialize(doc, …)` |
  | parsing | native: the arena is wrapped as the receiving class, then `initialize` runs with no arguments |

  Subclasses that override `initialize`, like Loofah's documents and
  fragments, therefore see the same calls they see under Nokogiri.
- **Moving to another document.** The node is copied into the new
  document and its object repointed at the copy. The original keeps its
  cache slot, so its old descendants, which keep their objects, still get
  the moved object from `parent`, as in Nokogiri
  (`test_a_node_moved_to_another_document`). The copy gets an object of
  its own, which the native method returns.
- **Reparenting text.** A text node is always copied, as Nokogiri copies
  it before libxml2 can merge it, and the original loses its cache slot.
  If the copy merges into neighboring text, the object is pointed at the
  node it merged into, which keeps its own object; both are `==`.
- **Copies.** `dup` and `clone` run `Object#dup`/`#clone`, so the copy
  gets the original's instance variables (and, for `clone`, singleton
  modules), then native `initialize_copy_with_args` fills in the copied
  node, caches and decorates it. `Document#dup` copies the arena's tree
  into a new one, with a new `@node_cache`; other instance variables,
  `@decorators` and `@errors` included, are shared, as in Nokogiri.
- **Lifetime.**
  - A node object marks its document, and the document owns every node it
    ever created. Unlinked nodes stay valid until the document is collected.
  - Nokogiri retains as much: it pins every node it unlinks
    (`noko_xml_document_pin_node`) until the document is freed, and its
    `@node_cache` holds every object it wrapped. So a document that is kept
    and edited grows with each node its edits create, about 100 bytes plus
    content here, under either library. The one exception is a text copy
    merged into its neighbor, which libxml2 frees; here it stays as an
    empty slot.
  - The `document` reference is marked movable and updated in `compact`.
  - A document reports its arena's heap size, unlinked nodes included, to
    `ObjectSpace.memsize_of`. Nokogiri's counts only the linked tree.
- **No borrow across Ruby.** The arena's `RefCell` is borrowed inside
  `with_arena`/`with_arena_mut` and released before any call into Ruby:
  decorate, `initialize`, blocks, CSS handlers. The closures can't reach
  Ruby:
  - they must be `Send`, and neither `&Ruby` nor `Value` is. `Opaque` and
    `Lazy` are, but reading one takes a `&Ruby`, so a closure can carry one
    and not use it;
  - the extension denies `unsafe` outside `support.rs`, which rules out
    raw rb-sys calls, and denies deprecated magnus API, which fetches a
    handle itself;
  - `ext/lobzique/clippy.toml` bans the rest: `Ruby::get`,
    `Ruby::get_with`, `eval` and the `eval!` and `rb_assert!` macros,
    `rb_sys::protect` outside `support.rs`, and `LazyId`, which is `Send`.
    CI runs clippy with warnings as errors.

  A Ruby exception therefore never unwinds past a held borrow, and a
  re-entrant call can't hit one. If one ever did, it would raise instead
  of panicking.
- **Strings.** Native methods take strings as Nokogiri hands them to
  libxml2 (`StringValueCStr`): converted with `to_str`, refused if they
  contain a NUL character, and stored as bytes without transcoding. They
  come back tagged UTF-8, valid or not.

### Threads, Ractors and panics

- **Threads.** Native methods hold the GVL, and Ruby switches threads only
  while Ruby code runs, which happens only with no borrow held. So other
  threads see a document before or after a native call, never during one.
- **Parsing** is the exception, unlike Nokogiri, which holds the GVL
  throughout. The parser builds in a document of its own and goes through
  its input 64 KB at a time (`DocumentParser`, `FragmentParser`). Between
  chunks, the extension lets Ruby handle interrupts and switch threads, as
  it does between Ruby calls (`support::check_interrupts`), so `Timeout`
  and `Thread#raise` stop a long parse.
  - From 1 MB of input it parses without the GVL instead
    (`support::without_gvl`), so other threads run meanwhile. An interrupt
    stops it after the current chunk, and it goes on from there unless the
    interrupt raised. Below 1 MB, getting the GVL back from a busy thread,
    up to that thread's 100 ms time slice, would cost more than the parse.
  - Code without the GVL can't reach Ruby: it is the `step` of a core type
    (`support::Resumable`, sealed), which can't hold a Ruby value.
  - The tree reaches the target document only after the parse, under a
    borrow taken then, in whatever document the fragment or context node is
    in by that time.

  XPath and CSS handlers run between evaluations
  (`XPathContext#evaluate`): if the handler or another thread edits the
  document, the next evaluation sees the edited tree with answers already
  given. Ids stay valid, so the result may be stale but nothing is unsafe.
- **Ractors.** Not supported, as in Nokogiri: the extension isn't declared
  Ractor-safe, so native methods raise `Ractor::UnsafeError` outside the
  main Ractor, and node types aren't `frozen_shareable`, so
  `Ractor.make_shareable` raises.
- **Panics** are bugs (the fuzzers look for them). magnus turns one in a
  native method into `fatal`: it can't be rescued, `ensure` and `at_exit`
  run, and the process exits. Unwinding releases the arena's borrow but
  can leave the arena half-edited; the core has no `unsafe`, so later
  calls can give wrong answers or panic again, not corrupt memory. A panic
  in `mark`, `free`, `size` or `compact` is an `rb_bug` crash.

### Other native classes

| class | role | Nokogiri counterpart |
|---|---|---|
| `Lobzique::Html5ever` | `parse(input, url, klass, **limits)`, `fragment(fragment, input, context, **limits)` and the `DEFAULT_MAX_*` limits | `Nokogiri::Gumbo` |
| `XML::Namespace` | a node's namespace: `prefix`, `href` | same |
| `XML::XPathContext` | `new(node)`, `node=`, `register_ns`, `register_variable`, `evaluate(path, handler)`: a `CSS::Selector` or an XPath string | same |
| `CSS::Selector` | `new(rule, prefixes:, doctype:, namespaces:, builtins:, first:)`: a selector list compiled for the implied XPath contexts of the searching class, as Nokogiri's `XPathVisitor` is configured | none; Nokogiri translates CSS into XPath strings |

## Core (`crates/lobzique-core`)

**Constraints.**
- No Ruby: magnus and rb-sys are not dependencies, so Miri, ASan and the
  fuzzers run the core alone.
- `#![forbid(unsafe_code)]`.
- Tree building, traversal, serialization and drop are iterative, so input
  depth can't overflow the stack.

**Naming.** The core uses Rust names for DOM concepts (`append_child`,
`descendants`). Its operations are the ones Nokogiri's primitives need, so
each native method stays a single call. For example, reparenting with
Nokogiri's copy-and-merge rules is one core operation. The ext does not
assemble it.

| module | holds |
|---|---|
| `dom` | `Document`, an arena of nodes addressed by `NodeId`; `NodeType` numbered as libxml2 numbers node types; `tree` (linking, unlinking, iterators, libxml2's sibling walks and document order); `attribute` (lookup by name, as each libxml2 reader does it); `namespace` (namespaces, definitions and scopes); `path` (`xmlGetNodePath`); `check` (invariants for tests and fuzzing) |
| `dom::reparent` | Nokogiri's `reparent_node_with` over libxml2 2.13: hierarchy checks, copying text and cross-document nodes, merging adjacent text (`xmlAddChild` and `replace` only), `relink_namespace` |
| `dom::copy` | `xmlDocCopyNode` at Nokogiri's three `dup` levels, `xmlCopyDoc`; a copy defines the foreign namespaces it uses on its root |
| `dom::text` | content, `native_content=` (`xmlNodeSetContent`); `blank?`, `encode_special_chars` |
| `html5::parse` | html5ever `TreeSink` into the arena: documents, fragments with a context node (appended to a fragment, or unlinked for `in_context`, whose context the core finds from the node; a fragment is parsed in a scratch document and copied in), a chunk of input at a time (`DocumentParser`, `FragmentParser`), `max_tree_depth`/`max_attributes`, quirks mode, errors; gumbo's text and CDATA nodes |
| `html5::serialize` | `html_standard_serialize`, byte for byte with Nokogiri::HTML5 |
| `xml::serialize` | `native_write_to`: libxml2's `xmlSaveTree` for XML and XHTML output, with its escaping, formatting, XHTML 1.0 appendix C rules and encoding switch; the binding transcodes the result |
| `css` | Nokogiri's CSS front end, ported: `tokenizer` (rex rules in order), `parser` (racc's parse loop over `tables`, which `script/generate_css_tables` generates from Nokogiri's parser, and the grammar's actions), `translate` (`XPathVisitor`, building `xpath` trees where it writes strings) |
| `xpath` | `ast` (expressions as trees, operators of one precedence as lists), `eval` (libxml2's evaluation: XPath 1.0 values, comparisons and functions, node sets in document order, positions per step, libxml2's evaluation errors, custom calls answered in rounds), `parse` (XPath strings read as libxml2 reads them, with its error codes); `XML::XPathContext` errors |
| `work` | a count of nodes read, bytes escaped and parser calls, kept only under `cfg(fuzzing)`, so `html5_scaling` can check that work grows linearly |

**Names.**
- Elements and attributes use html5ever's `QualName`, shared by the parser
  and the XPath engine, so names are UTF-8. Everything else (text,
  attribute values, doctype fields) is bytes.
- HTML elements are in the HTML namespace, following the HTML standard.
  `Node#namespace` shows that namespace as `nil`, as Nokogiri does.
- A parse defines each foreign namespace it uses on the top-level element
  above it (the `html` element of a document), as gumbo's binding does. So
  `namespaces` and prefixed lookups with `[]` answer as in Nokogiri. Nothing
  else defines namespaces.
- An element made with `Node.new` takes the document's namespace: HTML in
  HTML documents, none in XML documents.

## Departures from Nokogiri

| what | Nokogiri | lobzique | why |
|---|---|---|---|
| `NodeSet` storage | C methods over libxml2's `xmlNodeSet` | the same methods in Ruby over an Array (`xml/node_set.rb`); native results come through `NodeSet.wrap`, Nokogiri's `noko_xml_node_set_wrap` | nothing native is needed, and Ruby's GC already handles an Array |
| attribute values | text node children of the `Attr` node | bytes on the `Attr` node, which has no children; reparenting into an attribute raises "cannot reparent" | nothing in the contract reads an attribute's children; one fewer node per attribute |
| cycles | `reparent_node_with` raises after libxml2 has linked the cycle, leaving both nodes out of the document | raises the same error before changing anything | a corrupt tree helps nobody |
| a NodeSet after text is reparented | holds the original libxml2 node, which gets a new object, unlinked | holds the object, now pointing at the copy in the tree | a NodeSet holds objects, not nodes (`NodeSet` storage above) |
| `native_content=` with an entity reference other than the predefined five | an entity reference node, which `to_html` can't serialize | text, as written | no entity reference nodes; `content=` escapes `&`, so only a direct call reaches this |
| `create_element` with an `xmlns` key | a namespace definition | `NotImplementedError` | adding namespace definitions is out of scope |
| names that aren't UTF-8 | kept as bytes | `ArgumentError` | names are atoms shared with the parser and XPath engine; only `Node.new` can be given one |
| CSS | Ruby parser → XPath string → libxml2 | the same parser and visitor in the core, building an XPath tree (`CSS::Selector`) that the core evaluates as libxml2 would; `Searchable#extract_params` takes a `CSS::Selector` for a query, not a handler | no XPath string parser for the visitor's output; `CSS.xpath_for` stays out of scope (decision 2) |
| XPath's `namespace` axis | `XML::Namespace` objects in node sets | `XPath::UnsupportedError` (a `SyntaxError`, code 1207) | nothing in the inventory uses it (decision 2) |
| XPath nesting | unlimited | predicates, parentheses and argument lists at most 32 deep, else `XPath::UnsupportedError` | as for CSS: each level costs stack to parse and evaluate |
| XPath `id()` | libxml2's ID table: each `id` attribute (and `name` on `a`) as it was set, the first kept, unlinked nodes included | the first element with the ID in document order, in the document and then in the context node's tree | no ID table; the same for parsed documents |
| prefixes in XPath node tests | resolved when libxml2 reaches the step | all resolved before evaluating | differs only for a step never reached, which then raises 1219 |
| custom pseudo-class handlers | called per node from inside XPath evaluation | called per node with the same arguments, but in rounds between evaluations: an evaluation that reaches an unanswered call counts it as an empty node set and notes it, the noted calls run with the arena released, and evaluation starts again until none are left | no Ruby call while the arena is borrowed; a handler may run for a few nodes Nokogiri wouldn't call it for, which only side effects can tell. A call's error (no such method, `nil` returned) is raised only where an evaluation with every answer reaches it |
| errors in the visitor | `Racc::ParseError` (`:nth-child(2x)`), `NoMethodError` (`:foo(2n)`) | `CSS::SyntaxError` | the core has no Ruby errors to raise |
| `XPath::SyntaxError` text | quotes the XPath Nokogiri built, in the message and `str1` | quotes the CSS rule; class, domain and level as Nokogiri's | there is no XPath string |
| `XPath::SyntaxError` code for the XPath CSS becomes, when libxml2 can't parse it | the parse error libxml2 hits first: 1221 char out of XML range (`[x=a\b]`), 1206 invalid predicate (`[a\0]`), 1207 invalid expression, … | always 1207, "Invalid expression" | the translator knows the XPath is invalid, not where libxml2's parser would stop; codes for evaluation errors (1209, 1219, 1223) match |
| undefined namespace prefix | raised when libxml2 reaches the step | raised before evaluating | the same unless the step is never reached |
| nested parentheses in CSS | unlimited | at most 32 deep, else `CSS::SyntaxError` | the tree is recursive to translate and evaluate, about 4 KB of stack a level; real selectors nest two or three deep |
| CSS on HTML4 documents | names without a prefix match elements in no namespace, which is every element of an HTML4 parse | they match in any namespace, as in HTML5 documents | HTML4 names parse as HTML5 (decision 3), so `svg` has a namespace |
| HTML4 names | libxml2's HTML4 parser and serializer | the HTML5 parser and serializer, with libxml2's encoding handling (HTML4 names, above); `HTML5::Node`'s overrides apply to any `HTML4::Document`, which parses and serializes `noscript` content as markup; no HTML 4.0 doctype is added to parsed documents; parse options are ignored; html5ever's errors, at most 100 a parse, after libxml2's encoding warnings and errors | decision 3 |
| bytes invalid in an HTML4 document's encoding | raw in UTF-8 text, ISO-8859-1 in attributes; in an iconv encoding the parse stops | U+FFFD, and the parse goes on | the HTML5 parser takes only valid UTF-8; open, `docs/html4-invalid-bytes.md` |
| HTML4 input decoders | libxml2's and libiconv's | Ruby's transcoders, with libiconv's other names for them; a name neither knows is unsupported | the core has no transcoders; a few East Asian characters map differently (`test/differential/known.yml`) |
| `Node#parse` / `in_context` | libxml2's HTML4 parser, even in HTML5 documents; libxml2's XML parser in XML documents | the HTML5 fragment parser with the document's quirks mode and `parse_noscript_content_as_text`, in the context of the nearest element, or in `body` where there is none or it is in a namespace the parser has no rules for; html5ever's errors, at most 100 a parse as libxml2 keeps; `NotImplementedError` in XML documents | one parser; XML parsing is out of scope (decision 4) |
| `Node#parse` after errors with nothing parsed | parses again as a fragment without the context, in a new document | returns the empty NodeSet | the fallback emulates recovery for libxml2 < 2.13; the HTML5 parser always recovers |
| nodes `in_context` returns | unlinked, but each keeps its `previous_sibling` | unlinked from each other too | a stale link into nodes that aren't siblings |
| parse error text | gumbo's, with line and column | html5ever's, with line only | different parsers; listed in `test/differential/known.yml` if compared |
| libxml2's HTML serializer | `to_html` of nodes in XML documents, and save options that ask for it | `NotImplementedError` | HTML documents use the HTML5 serializer (decision 3); XML documents are out of scope (decision 4) |
| output encodings for `to_xml` and `to_xhtml` | libxml2's encoding handlers and names; output stops at invalid UTF-8 | Ruby's transcoders and encoding names, with `&#NNN;` for characters the encoding lacks, as libxml2 writes them; invalid UTF-8 is replaced; a name Ruby doesn't know writes nothing, as one libxml2 doesn't know does | the core has no transcoders; names differ only outside the common ones |
| `native_write_to` writes | one `io.write` per 4 KB | one `io.write` | only an IO object that counts its calls can tell |
| `<select>` contents | dropped, as the HTML standard did before 2025 | kept, as the standard and browsers now do (customizable select) | html5ever implements the current standard; `known.yml` lists the cases |
| `max_tree_depth` | counted on gumbo's stack of open elements | each element's depth where it is inserted, void elements excepted | html5ever doesn't expose its stack; the two agree on ordinary nesting and can differ after the adoption agency moves elements |
| `XML::Namespace` objects | one per libxml2 `xmlNs`, shared by the nodes that use it | a new object per call to `namespace`, `namespace_definitions` or `namespace_scopes` | the arena keeps namespaces as names, not objects; Loofah and rails-html-sanitizer read only `prefix` and `href` |
| `Node#pointer_id` | the libxml2 node's address | the address of the document's data, which doesn't move, and the node id | arena nodes have no address of their own; the value is still unique among live nodes, which is what `==` needs |
| native parser module | `Gumbo` | `Html5ever` | named after the engine it binds |
| threads while parsing | the GVL is held throughout; `Timeout` waits for the parse | interrupts are handled between 64 KB chunks, and from 1 MB the GVL is released | a long parse stalls neither the process nor `Timeout` (Threads, Ractors and panics) |

## How a call flows

- **Parsing.** `HTML5.parse(html)` → `HTML5::Document.parse` (Ruby: options,
  `HTML5.read_and_encode`) → `Html5ever.parse(string, url, self, …)` → the
  core builds an arena, a chunk at a time, without the GVL from 1 MB → it
  is wrapped as `self`, which may be Loofah's subclass → `initialize` →
  `@url`, `@quirks_mode`, `@errors`. `HTML4.parse(bytes)` →
  `HTML4::Document.parse` → `EncodingReader.detect_encoding` →
  `read_memory` → `EncodingReader.decode` → `Html5ever.parse` → the
  decoded `encoding` and libxml2's encoding errors.
- **Mutation.** `node.add_child("<b>x</b>")` → `coerce` → `fragment`, which
  parses in the context of `node` (`HTML5::Node#fragment`) →
  `add_child_node_and_reparent_attrs` → native `add_child_node` → core
  `reparent` → the wrapper is repointed if the node was copied →
  `decorate!`.
- **Serialization.** `node.to_html` → `to_format` → `serialize` →
  `HTML5::Node#write_to` → native `html_standard_serialize` → a UTF-8
  string, encoded into the document's encoding with `&#x…;` for unmappable
  characters.
- **XML and XHTML.** `doc.to_xhtml` → `to_format` → `serialize` →
  `HTML5::Node#write_to` (`XML::Node#write_to` in XML documents) → native
  `native_write_to` → core `xml::write`, escaped and formatted as libxml2
  does → transcoded into the requested encoding, or the document's when
  libxml2 would switch to it → `io.write`.
- **XPath.** `node.xpath("//p[@id]")` → `Searchable#xpath` →
  `xpath_internal` → `XPathContext#evaluate(string)` → the core parses
  the string into the same trees CSS compiles to, then evaluates it.
- **Search.** `node.css("p.x", handler)` → `Searchable#css` →
  `xpath_query_from_css_rule` → `CSS::Selector.new` (the core parses and
  translates it for the class's `IMPLIED_XPATH_CONTEXTS`) →
  `xpath_internal` → `XPathContext#evaluate(selector, handler)` → the core
  evaluates, and handler calls run between rounds → ids → `wrap` →
  `NodeSet`.

## Keeping the architecture honest

- `test/test_class_tree.rb` walks every constant under `Lobzique` and checks
  it against Nokogiri:
  - classes and modules must have the same ancestors, so `is_a?`, `case`
    and `super` resolve the same;
  - constants must have the same values;
  - every class, module and constant in `inventory/contract.yml` must exist.
- Behavior tests are written once and run against both roots: the same
  module is included in a `…Nokogiri` and a `…Lobzique` test case, as in
  `test/xml/test_object_model.rb`. An assertion that fails only on Nokogiri
  is a wrong test, not a pass.
- The differential harness compares whole documents and what selectors
  find in them (plan.md decision 6).
