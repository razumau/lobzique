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
    ever created.
  - Unlinked nodes stay valid until the document is collected, as libxml2
    nodes pinned by Nokogiri do.
  - The `document` reference is marked movable and updated in `compact`.
  - A document reports its arena's heap size to `ObjectSpace.memsize_of`.
- **No borrow across Ruby.** The arena's `RefCell` is borrowed inside
  `with_arena`/`with_arena_mut` and released before any call into Ruby:
  decorate, `initialize`, blocks, CSS handlers. The closures can't reach
  Ruby:
  - they must be `Send`, and Ruby handles and values aren't, so they can
    neither capture nor return one;
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

### Other native classes

| class | role | Nokogiri counterpart | status |
|---|---|---|---|
| `Lobzique::Html5ever` | `parse(input, url, klass, **limits)`, `fragment(fragment, input, context, **limits)` and the `DEFAULT_MAX_*` limits | `Nokogiri::Gumbo` | built |
| `XML::Namespace` | a node's namespace: `prefix`, `href` | same | built |
| `XML::XPathContext` | `new(node)`, `node=`, `register_ns`, `register_variable`, `evaluate(path, handler)` | same | built for `CSS::Selector`; XPath strings raise `NotImplementedError` until the subset (step 6) |
| `CSS::Selector` | `new(rule, prefixes:, doctype:, namespaces:, builtins:, first:)`: a selector list compiled for the implied XPath contexts of the searching class, as Nokogiri's `XPathVisitor` is configured | none; Nokogiri translates CSS into XPath strings | built |

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
| `dom` | `Document`, an arena of nodes addressed by `NodeId`; `NodeType` numbered as libxml2 numbers node types; `tree` (linking, unlinking, iterators, libxml2's sibling walks and document order); `attribute` (lookup by name, as each libxml2 reader does it); `namespace` (namespaces, definitions and scopes); `path` (`xmlGetNodePath`); `check` (invariants for tests and fuzzing) | built |
| `dom::reparent` | Nokogiri's `reparent_node_with` over libxml2 2.13: hierarchy checks, copying text and cross-document nodes, merging adjacent text (`xmlAddChild` and `replace` only), `relink_namespace` | built |
| `dom::copy` | `xmlDocCopyNode` at Nokogiri's three `dup` levels, `xmlCopyDoc`; a copy defines the foreign namespaces it uses on its root | built |
| `dom::text` | content, `native_content=` (`xmlNodeSetContent`); `blank?`, `encode_special_chars` | built |
| `html5::parse` | html5ever `TreeSink` into the arena: documents, fragments with a context node, `max_tree_depth`/`max_attributes`, quirks mode, errors; gumbo's text and CDATA nodes | built |
| `html5::serialize` | `html_standard_serialize`, byte for byte with Nokogiri::HTML5 | built |
| `xml::serialize` | `native_write_to` for `to_xhtml` | planned, late in phase 2 |
| `css` | Nokogiri's CSS front end, ported: `tokenizer` (rex rules in order), `parser` (racc's parse loop over `tables`, which `script/generate_css_tables` generates from Nokogiri's parser, and the grammar's actions), `translate` (`XPathVisitor`, building `xpath` trees where it writes strings) | built |
| `xpath` | `ast` (expressions as trees, `and`/`or` as lists), `eval` (libxml2's evaluation: XPath 1.0 values and comparisons, node sets in document order, positions per step, custom calls answered in rounds); `XML::XPathContext` errors | built; the parser for the location-path subset (plan.md decision 2) comes with step 6 |

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
| XPath | full XPath 1.0 in libxml2 | a native location-path subset behind the same `XPathContext` shape | decision 2 |
| custom pseudo-class handlers | called per node from inside XPath evaluation | called per node with the same arguments, but in rounds between evaluations: an evaluation that reaches an unanswered call counts it as an empty node set and notes it, the noted calls run with the arena released, and evaluation starts again until none are left | no Ruby call while the arena is borrowed; a handler may run for a few nodes Nokogiri wouldn't call it for, which only side effects can tell |
| errors in the visitor | `Racc::ParseError` (`:nth-child(2x)`), `ZeroDivisionError` (`0n-1`), `NoMethodError` (`:foo(2n)`) | `CSS::SyntaxError` | the core has no Ruby errors to raise |
| escapes of surrogates, code points past U+10FFFF and NUL (`#\D800`, `.\110000`, `[x=\0]`) | pasted into the XPath as invalid UTF-8 or a NUL byte: `ArgumentError` from a regexp or from evaluation, or `XPath::SyntaxError` "Char out of XML range" for a class | `CSS::SyntaxError` while compiling | names and values are Rust strings, which can hold neither; Nokogiri raises on each too, only later and with other classes |
| `XPath::SyntaxError` text | quotes the XPath Nokogiri built, in the message and `str1` | quotes the CSS rule; class, domain and level as Nokogiri's | there is no XPath string |
| `XPath::SyntaxError` code for XPath libxml2 can't parse | the parse error libxml2 hits first: 1202 unfinished literal (`[x=\"]`), 1206 invalid predicate (`#a\27`), 1207 invalid expression, … | always 1207, "Invalid expression" | the translator knows the XPath is invalid, not where libxml2's parser would stop; codes for evaluation errors (1209, 1219, 1223) match |
| undefined namespace prefix | raised when libxml2 reaches the step | raised before evaluating | the same unless the step is never reached |
| nested parentheses in CSS | unlimited | at most 32 deep, else `CSS::SyntaxError` | the tree is recursive to translate and evaluate, about 4 KB of stack a level; real selectors nest two or three deep |
| CSS on HTML4 documents | names without a prefix match elements in no namespace, which is every element of an HTML4 parse | they match in any namespace, as in HTML5 documents | HTML4 names parse as HTML5 (decision 3), so `svg` has a namespace |
| HTML4 names | libxml2's HTML4 parser and serializer | the HTML5 parser and serializer; `HTML5::Node`'s overrides apply to any `HTML4::Document` | decision 3 |
| `Node#parse` / `in_context` | libxml2's HTML4 parser, even in HTML5 documents | the HTML5 fragment parser with the node as context | one parser |
| parse error text | gumbo's, with line and column | html5ever's, with line only | different parsers; listed in `test/differential/known.yml` if compared |
| `<select>` contents | dropped, as the HTML standard did before 2025 | kept, as the standard and browsers now do (customizable select) | html5ever implements the current standard; `known.yml` lists the cases |
| `max_tree_depth` | counted on gumbo's stack of open elements | each element's depth where it is inserted, void elements excepted | html5ever doesn't expose its stack; the two agree on ordinary nesting and can differ after the adoption agency moves elements |
| `XML::Namespace` objects | one per libxml2 `xmlNs`, shared by the nodes that use it | a new object per call to `namespace`, `namespace_definitions` or `namespace_scopes` | the arena keeps namespaces as names, not objects; Loofah and rails-html-sanitizer read only `prefix` and `href` |
| `Node#pointer_id` | the libxml2 node's address | the address of the document's data, which doesn't move, and the node id | arena nodes have no address of their own; the value is still unique among live nodes, which is what `==` needs |
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
