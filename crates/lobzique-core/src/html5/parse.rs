//! html5ever's tree builder writing into the arena, with the limits and
//! output of Nokogiri's gumbo binding (`ext/nokogiri/gumbo.c`).

use super::{is_html, VOID_ELEMENTS};
use crate::dom::{CopyDepth, Doctype, Document, DocumentKind, NodeId, NodeKind, QualName};
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    self, BufferQueue, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use html5ever::tree_builder::{
    create_element, ElemName, ElementFlags, NodeOrText, TreeBuilder, TreeBuilderOpts, TreeSink,
};
use html5ever::{local_name, ns, Attribute, LocalName, Namespace, Prefix, TokenizerResult};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::fmt;

/// Limits as Nokogiri passes them to gumbo; `None` is no limit.
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub max_attributes: Option<usize>,
    pub max_errors: Option<usize>,
    pub max_tree_depth: Option<usize>,
    pub parse_noscript_content_as_text: bool,
}

/// Numbered as Nokogiri's `HTML5::QuirksMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum QuirksMode {
    NoQuirks = 0,
    Quirks = 1,
    LimitedQuirks = 2,
}

impl From<html5ever::tree_builder::QuirksMode> for QuirksMode {
    fn from(mode: html5ever::tree_builder::QuirksMode) -> QuirksMode {
        use html5ever::tree_builder::QuirksMode as Html5ever;
        match mode {
            Html5ever::NoQuirks => QuirksMode::NoQuirks,
            Html5ever::Quirks => QuirksMode::Quirks,
            Html5ever::LimitedQuirks => QuirksMode::LimitedQuirks,
        }
    }
}

impl From<QuirksMode> for html5ever::tree_builder::QuirksMode {
    fn from(mode: QuirksMode) -> html5ever::tree_builder::QuirksMode {
        use html5ever::tree_builder::QuirksMode as Html5ever;
        match mode {
            QuirksMode::NoQuirks => Html5ever::NoQuirks,
            QuirksMode::Quirks => Html5ever::Quirks,
            QuirksMode::LimitedQuirks => Html5ever::LimitedQuirks,
        }
    }
}

/// A limit was exceeded; gumbo then returns no tree. Displays as gumbo's
/// status string, which Nokogiri raises as an `ArgumentError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitError {
    TooManyAttributes,
    TreeTooDeep,
}

impl fmt::Display for LimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LimitError::TooManyAttributes => "Attributes per element limit exceeded",
            LimitError::TreeTooDeep => "Document tree depth limit exceeded",
        })
    }
}

impl std::error::Error for LimitError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub line: u64,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct Parsed {
    pub quirks_mode: QuirksMode,
    pub errors: Vec<ParseError>,
}

/// Parses a document. Input that isn't UTF-8 is decoded lossily, as gumbo
/// decodes it.
pub fn parse_document(input: &[u8], options: &Options) -> Result<(Document, Parsed), LimitError> {
    DocumentParser::new(input.to_vec(), options).finish()
}

/// A document parse that can be run a chunk of input at a time, so that a
/// caller can do something else between chunks.
pub struct DocumentParser(Run);

impl DocumentParser {
    pub fn new(input: Vec<u8>, options: &Options) -> DocumentParser {
        let sink = Sink::new(options, options.max_tree_depth);
        let builder = TreeBuilder::new(sink, tree_builder_options(options, QuirksMode::NoQuirks));
        DocumentParser(Run::new(builder, input, options, None))
    }

    /// Parses the next chunk of input; false once there is none left or a
    /// limit was exceeded.
    pub fn step(&mut self) -> bool {
        self.0.step()
    }

    /// Parses the rest of the input.
    pub fn finish(self) -> Result<(Document, Parsed), LimitError> {
        let (mut document, parsed) = self.0.finish()?;
        let roots: Vec<_> = document.element_children(NodeId::DOCUMENT).collect();
        for root in roots {
            define_namespaces(&mut document, root);
        }
        Ok((document, parsed))
    }
}

/// The context a fragment is parsed in, as Nokogiri's `Gumbo.fragment`
/// works it out from its `context` argument.
#[derive(Clone, Debug)]
pub struct Context {
    name: QualName,
    /// The `encoding` attribute of a MathML `annotation-xml` context.
    encoding: Option<Vec<u8>>,
    has_form_ancestor: bool,
    quirks_mode: QuirksMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextNamespace {
    Html,
    Svg,
    MathMl,
}

impl Context {
    /// Gumbo looks context names up without regard to case, so they are
    /// matched here in the case html5ever expects.
    pub fn new(namespace: ContextNamespace, name: &str) -> Context {
        let lower = name.to_ascii_lowercase();
        let (namespace, local) = match namespace {
            ContextNamespace::Html => (ns!(html), LocalName::from(lower)),
            ContextNamespace::Svg if lower == "foreignobject" => {
                (ns!(svg), local_name!("foreignObject"))
            }
            ContextNamespace::Svg => (ns!(svg), LocalName::from(lower)),
            ContextNamespace::MathMl => (ns!(mathml), LocalName::from(lower)),
        };
        Context {
            name: QualName::new(None, namespace, local),
            encoding: None,
            has_form_ancestor: false,
            quirks_mode: QuirksMode::NoQuirks,
        }
    }

    pub fn with_encoding(mut self, encoding: Option<Vec<u8>>) -> Context {
        self.encoding = encoding;
        self
    }

    pub fn with_form_ancestor(mut self, has_form_ancestor: bool) -> Context {
        self.has_form_ancestor = has_form_ancestor;
        self
    }

    pub fn with_quirks_mode(mut self, quirks_mode: QuirksMode) -> Context {
        self.quirks_mode = quirks_mode;
        self
    }

    /// `Gumbo.fragment`'s context for a node, whatever its kind: its name
    /// and namespace, an `annotation-xml`'s `encoding`, and whether it or
    /// an element above it is a `form`. The quirks mode is left to the
    /// caller, since `Gumbo.fragment` takes it from the fragment's document.
    pub fn of_node(document: &Document, node: NodeId) -> Result<Context, ContextError> {
        let name = document.node_name(node).ok_or(ContextError::NoName)?;
        let namespace = context_namespace(document, node).ok_or_else(|| {
            let href = document.namespace(node).map(|namespace| namespace.href);
            ContextError::UnknownNamespace(href.unwrap_or_default().to_owned())
        })?;
        let annotation_xml =
            namespace == ContextNamespace::MathMl && name.eq_ignore_ascii_case(b"annotation-xml");
        let encoding = annotation_xml
            .then(|| document.attribute_value(node, b"encoding"))
            .flatten();
        Ok(Context::new(namespace, &String::from_utf8_lossy(name))
            .with_encoding(encoding.map(<[u8]>::to_vec))
            .with_form_ancestor(has_form_ancestor(document, node)))
    }

    /// The context `Node#in_context` parses in: the nearest element at or
    /// above `node`, or `body` where there is none or it is in a namespace
    /// the parser has no rules for. The quirks mode is the doctype's if
    /// the document was `parsed`, as for `Gumbo.fragment` with a node.
    pub fn for_node(document: &Document, node: NodeId, parsed: bool) -> Context {
        let element = std::iter::once(node)
            .chain(document.ancestors(node))
            .find(|&id| matches!(document.node_kind(id), NodeKind::Element(_)));
        let body = Context::new(ContextNamespace::Html, "body");
        let context = match element {
            Some(element) => Context::of_node(document, element).unwrap_or_else(|_| {
                body.with_form_ancestor(has_form_ancestor(document, element))
            }),
            None => body,
        };
        let quirks_mode = if parsed {
            doctype_quirks_mode(document)
        } else {
            QuirksMode::NoQuirks
        };
        context.with_quirks_mode(quirks_mode)
    }
}

/// Why `Context::of_node` refuses a node, as `Gumbo.fragment` does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextError {
    /// A document, fragment or CDATA section, whose `name` is nil.
    NoName,
    /// A namespace the parser has no rules for, by its href.
    UnknownNamespace(String),
}

/// The quirks mode of a fragment parsed in the context of a node of the
/// parsed `document`: the one its doctype selects, or quirks without one.
pub fn doctype_quirks_mode(document: &Document) -> QuirksMode {
    let doctype = document
        .doctype()
        .and_then(|id| match document.node_kind(id) {
            NodeKind::Doctype(doctype) => Some(doctype),
            _ => None,
        });
    match doctype {
        None => QuirksMode::Quirks,
        Some(doctype) => quirks_mode_for_doctype(
            Some(&doctype.name),
            doctype.public_id.as_deref(),
            doctype.system_id.as_deref(),
        ),
    }
}

/// gumbo.c's `lookup_namespace` for an element; `None` for a namespace the
/// parser has no rules for.
fn context_namespace(document: &Document, element: NodeId) -> Option<ContextNamespace> {
    let Some(namespace) = document.namespace(element) else {
        return Some(ContextNamespace::Html);
    };
    match namespace.href {
        "http://www.w3.org/1999/xhtml" => Some(ContextNamespace::Html),
        "http://www.w3.org/1998/Math/MathML" => Some(ContextNamespace::MathMl),
        "http://www.w3.org/2000/svg" => Some(ContextNamespace::Svg),
        _ => None,
    }
}

/// Whether `node` or an element above it is an HTML `form`.
fn has_form_ancestor(document: &Document, node: NodeId) -> bool {
    let is_form = |id| {
        matches!(document.node_kind(id), NodeKind::Element(element)
            if element.name.local.eq_ignore_ascii_case(&local_name!("form")))
    };
    std::iter::once(node)
        .chain(document.ancestors(node))
        .any(|id| is_form(id) && context_namespace(document, id) == Some(ContextNamespace::Html))
}

/// Parses a fragment in `context` and appends what it makes to `fragment`.
pub fn parse_fragment(
    document: &mut Document,
    fragment: NodeId,
    input: &[u8],
    context: &Context,
    options: &Options,
) -> Result<Parsed, LimitError> {
    let parsed = FragmentParser::new(input.to_vec(), context, options).finish()?;
    Ok(parsed.append_to(document, fragment))
}

/// Parses a fragment in `context` and returns its top-level nodes, each
/// unlinked from its parent and siblings.
pub fn parse_in_context(
    document: &mut Document,
    input: &[u8],
    context: &Context,
    options: &Options,
) -> Result<(Vec<NodeId>, Parsed), LimitError> {
    let parsed = FragmentParser::new(input.to_vec(), context, options).finish()?;
    Ok(parsed.import(document))
}

/// A fragment parse that can be run a chunk of input at a time, like
/// [`DocumentParser`]. It builds in a scratch document of its own, so the
/// parser's scaffolding (an html root, the context and form elements) and
/// the nodes of a parse that fails never reach the document they are for.
pub struct FragmentParser(Run);

impl FragmentParser {
    pub fn new(input: Vec<u8>, context: &Context, options: &Options) -> FragmentParser {
        // The tree has an html element above the fragment's nodes, as gumbo's does
        let max_tree_depth = options.max_tree_depth.map(|depth| depth.saturating_add(1));
        let sink = Sink::new(options, max_tree_depth);
        // The tree builder starts in this mode and never sets it in a fragment
        sink.quirks_mode.set(context.quirks_mode);

        let mut attributes = Vec::new();
        if let Some(encoding) = &context.encoding {
            attributes.push(Attribute {
                name: QualName::new(None, ns!(), local_name!("encoding")),
                value: StrTendril::from_slice(&String::from_utf8_lossy(encoding)),
            });
        }
        let context_element = create_element(&sink, context.name.clone(), attributes);
        let form = context.has_form_ancestor.then(|| {
            let name = QualName::new(None, ns!(html), local_name!("form"));
            create_element(&sink, name, Vec::new())
        });
        let builder = TreeBuilder::new_for_fragment(
            sink,
            context_element,
            form,
            tree_builder_options(options, context.quirks_mode),
        );
        let initial_state =
            builder.tokenizer_state_for_context_elem(options.parse_noscript_content_as_text);
        FragmentParser(Run::new(builder, input, options, Some(initial_state)))
    }

    /// Parses the next chunk of input; false once there is none left or a
    /// limit was exceeded.
    pub fn step(&mut self) -> bool {
        self.0.step()
    }

    /// Parses the rest of the input.
    pub fn finish(self) -> Result<ParsedFragment, LimitError> {
        let (scratch, parsed) = self.0.finish()?;
        Ok(ParsedFragment { scratch, parsed })
    }
}

/// A fragment's nodes, still in the parser's scratch document.
pub struct ParsedFragment {
    scratch: Document,
    parsed: Parsed,
}

impl ParsedFragment {
    /// Copies the top-level nodes into `document`, unlinked. The copies get
    /// the names and namespace definitions `document` writes.
    pub fn import(self, document: &mut Document) -> (Vec<NodeId>, Parsed) {
        let scratch = &self.scratch;
        let root = scratch.first_child(NodeId::DOCUMENT);
        let nodes = root
            .into_iter()
            .flat_map(|root| scratch.children(root))
            .filter_map(|child| document.import(scratch, child, CopyDepth::Deep))
            .collect();
        (nodes, self.parsed)
    }

    /// Copies the top-level nodes into `document` and appends them to
    /// `fragment`.
    pub fn append_to(self, document: &mut Document, fragment: NodeId) -> Parsed {
        let (nodes, parsed) = self.import(document);
        for node in nodes {
            if document.append_child(fragment, node).is_err() {
                break;
            }
        }
        parsed
    }
}

/// Gumbo's binding defines the foreign namespaces a top-level element and
/// its descendants use on that element, so they are in scope as libxml2
/// needs them to be.
fn define_namespaces(document: &mut Document, root: NodeId) {
    let mut used = Vec::new();
    for element in std::iter::once(root).chain(document.descendants(root)) {
        let attributes = document.attributes(element).iter().copied();
        for node in std::iter::once(element).chain(attributes) {
            let Some(namespace) = document.namespace(node) else {
                continue;
            };
            // libxml2 binds `xml` without a definition
            let Some(prefix) = namespace.prefix.filter(|&prefix| prefix != "xml") else {
                continue;
            };
            if used
                .iter()
                .all(|(defined, _): &(Prefix, _)| **defined != *prefix)
            {
                used.push((Prefix::from(prefix), Namespace::from(namespace.href)));
            }
        }
    }
    for (prefix, href) in used {
        document.define_namespace(root, Some(prefix), href);
    }
}

/// The quirks mode a doctype selects, which Nokogiri computes for a
/// fragment parsed in the context of a node of a parsed document.
pub fn quirks_mode_for_doctype(
    name: Option<&[u8]>,
    public_id: Option<&[u8]>,
    system_id: Option<&[u8]>,
) -> QuirksMode {
    let tendril = |bytes: Option<&[u8]>| {
        bytes.map(|bytes| StrTendril::from_slice(&String::from_utf8_lossy(bytes)))
    };
    let doctype = tokenizer::Doctype {
        name: tendril(name),
        public_id: tendril(public_id),
        system_id: tendril(system_id),
        force_quirks: false,
    };
    // A tree builder in its initial mode decides the quirks mode from the
    // doctype token, with the same table it uses for documents
    let options = Options::default();
    let sink = Sink::new(&options, None);
    let builder = TreeBuilder::new(sink, tree_builder_options(&options, QuirksMode::NoQuirks));
    let _ = builder.process_token(Token::DoctypeToken(doctype), 1);
    builder.sink.quirks_mode.get()
}

fn tree_builder_options(options: &Options, quirks_mode: QuirksMode) -> TreeBuilderOpts {
    TreeBuilderOpts {
        // Gumbo parses noscript content as markup unless asked not to
        scripting_enabled: options.parse_noscript_content_as_text,
        quirks_mode: quirks_mode.into(),
        ..TreeBuilderOpts::default()
    }
}

/// Input goes to the tokenizer in pieces, so parsing stops soon after a
/// limit is exceeded and no tendril outgrows its 32-bit length. Smaller
/// under Miri, so tests can cross chunks in reasonable time.
pub(super) const CHUNK: usize = if cfg!(miri) { 1 << 8 } else { 1 << 16 };

/// The tokenizer, the tree builder it feeds, and the input not fed yet.
struct Run {
    tokenizer: Tokenizer<Tokens>,
    queue: BufferQueue,
    input: String,
    fed: usize,
}

impl Run {
    fn new(
        builder: TreeBuilder<NodeId, Sink>,
        input: Vec<u8>,
        options: &Options,
        initial_state: Option<tokenizer::states::State>,
    ) -> Run {
        let input = String::from_utf8(input)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned());
        let tokenizer = Tokenizer::new(
            Tokens {
                builder,
                max_attributes: options.max_attributes,
                exceeded: Cell::new(false),
            },
            TokenizerOpts {
                initial_state,
                ..TokenizerOpts::default()
            },
        );
        Run {
            tokenizer,
            queue: BufferQueue::default(),
            input,
            fed: 0,
        }
    }

    fn step(&mut self) -> bool {
        let rest = &self.input[self.fed..];
        if rest.is_empty() || self.tokenizer.sink.limit_error().is_some() {
            return false;
        }
        let mut end = rest.len().min(CHUNK);
        while !rest.is_char_boundary(end) {
            end += 1;
        }
        self.queue.push_back(StrTendril::from_slice(&rest[..end]));
        self.fed += end;
        while !matches!(self.tokenizer.feed(&self.queue), TokenizerResult::Done) {}
        true
    }

    fn finish(mut self) -> Result<(Document, Parsed), LimitError> {
        while self.step() {}
        self.tokenizer.end();

        let tokens = self.tokenizer.sink;
        if let Some(error) = tokens.limit_error() {
            return Err(error);
        }
        let sink = tokens.builder.sink;
        let parsed = Parsed {
            quirks_mode: sink.quirks_mode.get(),
            errors: sink.errors.into_inner(),
        };
        Ok((sink.document.into_inner(), parsed))
    }
}

/// Passes tokens to the tree builder, counting attributes on the way.
/// Gumbo's tokenizer refuses a tag with more distinct attributes than the
/// limit, and html5ever's has already dropped the duplicates.
struct Tokens {
    builder: TreeBuilder<NodeId, Sink>,
    max_attributes: Option<usize>,
    exceeded: Cell<bool>,
}

impl Tokens {
    fn limit_error(&self) -> Option<LimitError> {
        if self.exceeded.get() {
            Some(LimitError::TooManyAttributes)
        } else if self.builder.sink.too_deep.get() {
            Some(LimitError::TreeTooDeep)
        } else {
            None
        }
    }
}

impl TokenSink for Tokens {
    type Handle = NodeId;

    fn process_token(&self, token: Token, line_number: u64) -> TokenSinkResult<NodeId> {
        crate::work::add(1);
        if let Token::TagToken(tag) = &token {
            if self.max_attributes.is_some_and(|max| tag.attrs.len() > max) {
                self.exceeded.set(true);
            }
        }
        self.builder.process_token(token, line_number)
    }

    fn end(&self) {
        self.builder.end();
    }

    /// The tokenizer asks this at `<![`, and opens a CDATA section if the
    /// answer is yes and `CDATA[` follows.
    fn adjusted_current_node_present_but_not_in_html_namespace(&self) -> bool {
        self.builder
            .adjusted_current_node_present_but_not_in_html_namespace()
    }
}

/// Builds the tree in the arena. A template's contents are its children,
/// as Nokogiri builds them.
struct Sink {
    document: RefCell<Document>,
    quirks_mode: Cell<QuirksMode>,
    line: Cell<u64>,
    errors: RefCell<Vec<ParseError>>,
    max_errors: Option<usize>,
    max_tree_depth: Option<usize>,
    too_deep: Cell<bool>,
    /// MathML `annotation-xml` elements whose content parses as HTML.
    integration_points: RefCell<HashSet<NodeId>>,
}

impl Sink {
    fn new(options: &Options, max_tree_depth: Option<usize>) -> Sink {
        Sink {
            document: RefCell::new(Document::new(DocumentKind::Html)),
            quirks_mode: Cell::new(QuirksMode::NoQuirks),
            line: Cell::new(1),
            errors: RefCell::new(Vec::new()),
            max_errors: options.max_errors,
            max_tree_depth,
            too_deep: Cell::new(false),
            integration_points: RefCell::default(),
        }
    }

    /// Gumbo's limit is on its stack of open elements, which html5ever
    /// doesn't expose. Its depth is that of the element last pushed, so an
    /// element is measured where it is inserted. Void elements are never
    /// pushed. Elements the adoption agency moves are measured again, which
    /// gumbo doesn't do.
    fn check_depth(&self, document: &Document, parent: NodeId, child: NodeId) {
        let Some(max) = self.max_tree_depth else {
            return;
        };
        if self.too_deep.get() {
            return;
        }
        let NodeKind::Element(element) = document.node_kind(child) else {
            return;
        };
        if is_html(&element.name) && VOID_ELEMENTS.contains(&&*element.name.local) {
            return;
        }
        let depth = std::iter::once(parent)
            .chain(document.ancestors(parent))
            .filter(|&id| matches!(document.node_kind(id), NodeKind::Element(_)))
            .take(max)
            .count()
            + 1;
        if depth > max {
            self.too_deep.set(true);
        }
    }

    /// Inserts a node or text into `parent`, before `before` or at the end.
    fn insert(&self, parent: NodeId, before: Option<NodeId>, child: NodeOrText<NodeId>) {
        let mut document = self.document.borrow_mut();
        let node = match child {
            NodeOrText::AppendNode(node) => {
                self.check_depth(&document, parent, node);
                node
            }
            NodeOrText::AppendText(text) => {
                match self.insert_text(&mut document, parent, before, text.as_bytes()) {
                    Some(node) => node,
                    None => return,
                }
            }
        };
        // html5ever only asks for insertions the DOM allows
        let _ = match before {
            Some(sibling) => document.insert_before(sibling, node),
            None => document.append_child(parent, node),
        };
    }

    /// Adjacent character tokens make one text node. A CDATA section's
    /// characters are text too, as in the HTML standard: gumbo makes a
    /// CDATA node of them and of the text around them, which is serialized
    /// raw. Returns a new node to insert, or `None` if the text went into
    /// the node next to it.
    fn insert_text(
        &self,
        document: &mut Document,
        parent: NodeId,
        before: Option<NodeId>,
        text: &[u8],
    ) -> Option<NodeId> {
        crate::work::add(text.len());
        if text.is_empty() {
            return None;
        }
        let neighbour = match before {
            Some(sibling) => document.previous_sibling(sibling),
            None => document.last_child(parent),
        };
        match neighbour.and_then(|id| match document.node_kind(id) {
            NodeKind::Text(_) => document.character_data_mut(id),
            _ => None,
        }) {
            Some(data) => {
                data.extend_from_slice(text);
                None
            }
            None => Some(document.create_text(text.to_vec())),
        }
    }
}

/// An element's name, copied out of the arena so no borrow outlives the call.
#[derive(Debug)]
struct Name(QualName);

impl ElemName for Name {
    fn ns(&self) -> &Namespace {
        &self.0.ns
    }

    fn local_name(&self) -> &LocalName {
        &self.0.local
    }
}

impl TreeSink for Sink {
    type Handle = NodeId;
    type Output = ();
    type ElemName<'b>
        = Name
    where
        Self: 'b;

    fn finish(self) {}

    fn parse_error(&self, message: Cow<'static, str>) {
        crate::work::add(1);
        let mut errors = self.errors.borrow_mut();
        if self.max_errors.is_none_or(|max| errors.len() < max) {
            errors.push(ParseError {
                line: self.line.get(),
                message: message.into_owned(),
            });
        }
    }

    fn get_document(&self) -> NodeId {
        NodeId::DOCUMENT
    }

    fn elem_name(&self, target: &NodeId) -> Name {
        match self.document.borrow().node_kind(*target) {
            NodeKind::Element(element) => Name(element.name.clone()),
            _ => Name(QualName::new(None, ns!(), local_name!(""))),
        }
    }

    fn create_element(
        &self,
        name: QualName,
        attributes: Vec<Attribute>,
        flags: ElementFlags,
    ) -> NodeId {
        let mut document = self.document.borrow_mut();
        let element = document.create_element(name);
        for attribute in attributes {
            let node =
                document.create_attribute(attribute.name, attribute.value.as_bytes().to_vec());
            let _ = document.append_attribute(element, node);
        }
        if flags.mathml_annotation_xml_integration_point {
            self.integration_points.borrow_mut().insert(element);
        }
        element
    }

    fn create_comment(&self, text: StrTendril) -> NodeId {
        self.document
            .borrow_mut()
            .create_comment(text.as_bytes().to_vec())
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> NodeId {
        self.document
            .borrow_mut()
            .create_processing_instruction(target.as_bytes().to_vec(), data.as_bytes().to_vec())
    }

    fn append(&self, parent: &NodeId, child: NodeOrText<NodeId>) {
        self.insert(*parent, None, child);
    }

    fn append_based_on_parent_node(
        &self,
        element: &NodeId,
        prev_element: &NodeId,
        child: NodeOrText<NodeId>,
    ) {
        let has_parent = self.document.borrow().parent(*element).is_some();
        if has_parent {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    /// libxml2 makes the doctype the document's first child, wherever it was
    /// in the input.
    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        let optional = |id: StrTendril| (!id.is_empty()).then(|| id.as_bytes().to_vec());
        let mut document = self.document.borrow_mut();
        let doctype = document.create_doctype(Doctype {
            name: name.as_bytes().to_vec(),
            public_id: optional(public_id),
            system_id: optional(system_id),
        });
        let _ = match document.first_child(NodeId::DOCUMENT) {
            Some(first) => document.insert_before(first, doctype),
            None => document.append_child(NodeId::DOCUMENT, doctype),
        };
    }

    fn get_template_contents(&self, target: &NodeId) -> NodeId {
        *target
    }

    fn same_node(&self, x: &NodeId, y: &NodeId) -> bool {
        crate::work::add(1);
        x == y
    }

    fn set_quirks_mode(&self, mode: html5ever::tree_builder::QuirksMode) {
        self.quirks_mode.set(mode.into());
    }

    fn append_before_sibling(&self, sibling: &NodeId, new_node: NodeOrText<NodeId>) {
        let parent = self.document.borrow().parent(*sibling);
        if let Some(parent) = parent {
            self.insert(parent, Some(*sibling), new_node);
        }
    }

    fn add_attrs_if_missing(&self, target: &NodeId, attributes: Vec<Attribute>) {
        let mut document = self.document.borrow_mut();
        for attribute in attributes {
            let present = document.attributes(*target).iter().any(|&existing| {
                matches!(document.node_kind(existing),
                    NodeKind::Attribute(a) if a.name == attribute.name)
            });
            if !present {
                let node =
                    document.create_attribute(attribute.name, attribute.value.as_bytes().to_vec());
                let _ = document.append_attribute(*target, node);
            }
        }
    }

    fn remove_from_parent(&self, target: &NodeId) {
        self.document.borrow_mut().detach(*target);
    }

    fn reparent_children(&self, node: &NodeId, new_parent: &NodeId) {
        let mut document = self.document.borrow_mut();
        while let Some(child) = document.first_child(*node) {
            if document.append_child(*new_parent, child).is_err() {
                break;
            }
        }
    }

    fn is_mathml_annotation_xml_integration_point(&self, handle: &NodeId) -> bool {
        crate::work::add(1);
        self.integration_points.borrow().contains(handle)
    }

    fn set_current_line(&self, line_number: u64) {
        self.line.set(line_number);
    }
}
