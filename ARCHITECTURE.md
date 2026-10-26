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
  parsing, namespace definitions, XInclude, C14N, SAX, XSLT.
- JRuby branches are dropped.

**Departures are marked.** Every change to ported code has a comment starting
with `# lobzique:`. `grep -rn "lobzique:" lib` lists everything to recheck
when the Nokogiri version in `Gemfile.lock` moves.

**Not ported.**
- `css/parser`, `tokenizer`, `xpath_visitor` and `css/node`: CSS is matched
  natively instead of being translated to XPath.
- `html4/encoding_reader`: the HTML4 names use the HTML5 parser's encoding
  rules.
- SAX, XSLT, Builder, Reader, Schema and RelaxNG.

**Facade-safe.**
- Code inside `module Lobzique` refers to constants relatively or through
  `Lobzique`.
- Messages are built from classes (`"must be a #{XML::Node}"`), never from a
  literal root name.
- So a facade that sets `Lobzique = Nokogiri` gets `Nokogiri::…` in every
  message (plan.md decision 1).

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
only in Ruby.

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
  (`test_a_node_moved_to_another_document`).
- **Lifetime.**
  - A node object marks its document, and the document owns every node it
    ever created.
  - Unlinked nodes stay valid until the document is collected, as libxml2
    nodes pinned by Nokogiri do.
  - The `document` reference is marked movable and updated in `compact`.
  - A document reports its arena's heap size to `ObjectSpace.memsize_of`.
- **No borrow across Ruby.** The arena's `RefCell` is borrowed inside
  `with_arena`/`with_arena_mut` and released before any call into Ruby:
  decorate, `initialize`, blocks, CSS handlers. The closures must be
  `Send`, and Ruby handles and values aren't, so the compiler rejects a
  closure that calls Ruby or returns a Ruby object. A Ruby exception
  therefore never unwinds past a held borrow, and a re-entrant call can't
  hit one. If one ever did, it would raise instead of panicking.
- **Strings.** Native methods take strings as Nokogiri hands them to
  libxml2 (`StringValueCStr`): converted with `to_str`, refused if they
  contain a NUL character, and stored as bytes without transcoding. They
  come back tagged UTF-8, valid or not.

### Planned native classes

| class | role | Nokogiri counterpart |
|---|---|---|
| `Lobzique::Html5ever` | `parse(input, url, klass, **limits)`, `fragment(fragment, input, context, **limits)` and the `DEFAULT_MAX_*` limits | `Nokogiri::Gumbo` |
| `XML::XPathContext` | `new(node)`, `evaluate(path, handler)` for the XPath subset | same |
| `CSS::Selector` | a compiled selector list: `search(node, include_self, resolved_handlers)` | none; Nokogiri compiles CSS to XPath |

Each arrives with the phase 2 step that needs it (plan.md).

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

| module | holds | status |
|---|---|---|
| `dom` | `Document`, an arena of nodes addressed by `NodeId`; `NodeType` numbered as libxml2 numbers node types; `tree` (linking, unlinking, iterators); `check` (invariants for tests and fuzzing) | built |
| `dom::reparent` | Nokogiri's `reparent_node_with` rules: hierarchy checks, copying text and cross-document nodes, merging adjacent text | planned |
| `dom::text` | content, `blank?`, `encode_special_chars` | planned |
| `html5::parse` | html5ever `TreeSink` into the arena: documents, fragments with a context node, `max_tree_depth`/`max_attributes`, quirks mode, errors | planned |
| `html5::serialize` | `html_standard_serialize`, byte for byte with Nokogiri::HTML5 | planned |
| `xml::serialize` | `native_write_to` for `to_xhtml` | planned, late in phase 2 |
| `css` | Servo `selectors` over the arena; `dialect` for Nokogiri's extensions (`[@attr]`, `:first`, …) | planned |
| `xpath` | parser and evaluator for the location-path subset; everything else is an error naming the construct (plan.md decision 2) | planned |

**Names.**
- Elements and attributes use html5ever's `QualName`, shared by the parser
  and the selector engine, so names are UTF-8. Everything else (text,
  attribute values, doctype fields) is bytes.
- HTML elements are in the HTML namespace, following the HTML standard.
  `Node#namespace` shows that namespace as `nil`, as Nokogiri does.
- An element made with `Node.new` takes the document's namespace: HTML in
  HTML documents, none in XML documents.

## Departures from Nokogiri

| what | Nokogiri | lobzique | why |
|---|---|---|---|
| `NodeSet` storage | C methods over libxml2's `xmlNodeSet` | the same methods in Ruby over an Array (`xml/node_set.rb`); native results come through `NodeSet.wrap`, Nokogiri's `noko_xml_node_set_wrap` | nothing native is needed, and Ruby's GC already handles an Array |
| attribute values | text node children of the `Attr` node | bytes on the `Attr` node | nothing in the contract reads an attribute's children; one fewer node per attribute |
| names that aren't UTF-8 | kept as bytes | `ArgumentError` | names are atoms shared with the parser and selector engine; only `Node.new` can be given one |
| CSS | Ruby parser → XPath → libxml2 | native selector engine behind `Searchable#css` | no XPath engine to compile to; `CSS.xpath_for` stays out of scope (decision 2) |
| XPath | full XPath 1.0 in libxml2 | a native location-path subset behind the same `XPathContext` shape | decision 2 |
| custom pseudo-class handlers | called per node from inside XPath evaluation | called once per pseudo-class with the candidate NodeSet, before matching; the id sets go into the match | no Ruby call while the arena is borrowed; the handlers in rails-dom-testing and Capybara filter sets either way |
| HTML4 names | libxml2's HTML4 parser and serializer | the HTML5 parser and serializer; `HTML5::Node`'s overrides apply to any `HTML4::Document` | decision 3 |
| `Node#parse` / `in_context` | libxml2's HTML4 parser, even in HTML5 documents | the HTML5 fragment parser with the node as context | one parser |
| parse error text | gumbo's | html5ever's | different parsers; listed in `test/differential/known.yml` if compared |
| native parser module | `Gumbo` | `Html5ever` | named after the engine it binds |

## How a call flows

- **Parsing.** `HTML5.parse(html)` → `HTML5::Document.parse` (Ruby: options,
  `HTML5.read_and_encode`) → `Html5ever.parse(string, url, self, …)` → the
  core builds an arena → it is wrapped as `self`, which may be Loofah's
  subclass → `initialize` → `@url`, `@quirks_mode`, `@errors`.
- **Mutation.** `node.add_child("<b>x</b>")` → `coerce` → `fragment`, which
  parses in the context of `node` (`HTML5::Node#fragment`) →
  `add_child_node_and_reparent_attrs` → native `add_child_node` → core
  `reparent` → the wrapper is repointed if the node was copied →
  `decorate!`.
- **Serialization.** `node.to_html` → `to_format` → `serialize` →
  `HTML5::Node#write_to` → native `html_standard_serialize` → a UTF-8
  string, encoded into the document's encoding with `&#x…;` for unmappable
  characters.
- **Search.** `node.css("p.x", handler)` → `Searchable#css` →
  `CSS::Selector` (compiled once, cached) → the handler resolves custom
  pseudo-classes → native `search` → ids → `wrap` → `NodeSet`.

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
- The differential harness compares whole documents (plan.md decision 6).
