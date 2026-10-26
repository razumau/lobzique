//! `XML::Node`, and the wrapper every node object shares, documents included.

use super::ELEMENT;
use super::{
    attr::ATTR, cdata::CDATA, comment::COMMENT, document, document_fragment::DOCUMENT_FRAGMENT,
    dtd::DTD, text::TEXT,
};
use crate::support::{call_initialize, warn_deprecated, yield_to_block};
use crate::XML;
use lobzique_core::dom::{self, NodeId, NodeType};
use magnus::{
    gc, method,
    prelude::*,
    typed_data::Obj,
    value::{Lazy, Opaque},
    DataTypeFunctions, Error, RArray, RClass, RString, Ruby, TypedData, Value,
};
use std::cell::{Cell, OnceCell, RefCell};

pub static NODE: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .define_class("Node", ruby.class_object())
        .unwrap()
});

/// The data behind every node object. A node is its document plus its id in
/// that document's arena; both change when the node moves to another
/// document. A document is the node that owns the arena, so `Document <
/// Node` works without a second data type.
#[derive(Default, TypedData)]
#[magnus(class = "Lobzique::XML::Node", mark, compact, size, free_immediately)]
pub struct Node {
    /// `None` only on an object made with `allocate` and never initialized.
    document: Cell<Option<Opaque<Value>>>,
    id: Cell<Option<NodeId>>,
    arena: OnceCell<RefCell<dom::Document>>,
}

impl DataTypeFunctions for Node {
    fn mark(&self, marker: &gc::Marker) {
        if let Some(document) = self.document.get() {
            marker.mark_movable(document);
        }
    }

    fn compact(&self, compactor: &gc::Compactor) {
        if let Some(document) = self.document.get() {
            self.document.set(Some(compactor.location(document)));
        }
    }
}

impl Node {
    fn new(document: Value, id: NodeId) -> Node {
        Node {
            document: Cell::new(Some(document.into())),
            id: Cell::new(Some(id)),
            arena: OnceCell::new(),
        }
    }

    /// The document node's own data, which owns the arena.
    pub fn new_document(arena: dom::Document) -> Node {
        Node {
            document: Cell::new(None),
            id: Cell::new(Some(NodeId::DOCUMENT)),
            arena: OnceCell::from(RefCell::new(arena)),
        }
    }

    /// Completes a document node's data once its Ruby object exists.
    pub fn set_document(&self, document: Value) {
        self.document.set(Some(document.into()));
    }

    pub fn arena(&self) -> Option<&RefCell<dom::Document>> {
        self.arena.get()
    }

    pub fn document(&self, ruby: &Ruby) -> Result<Value, Error> {
        self.document
            .get()
            .map(|document| ruby.get_inner(document))
            .ok_or_else(|| uninitialized(ruby))
    }

    pub fn id(&self, ruby: &Ruby) -> Result<NodeId, Error> {
        self.id.get().ok_or_else(|| uninitialized(ruby))
    }

    /// Runs `f` on the node's arena. The borrow ends when `f` returns, before
    /// anything reaches Ruby (plan.md decision 5).
    pub fn with_arena<R>(
        &self,
        ruby: &Ruby,
        f: impl FnOnce(&dom::Document, NodeId) -> R,
    ) -> Result<R, Error> {
        let id = self.id(ruby)?;
        document::with_arena(ruby, self.document(ruby)?, |arena| f(arena, id))
    }

    fn node_type(ruby: &Ruby, rb_self: &Self) -> Result<u8, Error> {
        rb_self.with_arena(ruby, |arena, id| arena.node_type(id) as u8)
    }

    fn node_name(ruby: &Ruby, rb_self: &Self) -> Result<Option<RString>, Error> {
        rb_self.with_arena(ruby, |arena, id| {
            arena.node_name(id).map(|name| ruby.str_new(name))
        })
    }

    fn data_ptr_p(&self) -> bool {
        self.document.get().is_some()
    }

    fn document_method(ruby: &Ruby, rb_self: &Self) -> Result<Value, Error> {
        rb_self.document(ruby)
    }

    fn internal_subset(ruby: &Ruby, rb_self: &Self) -> Result<Option<Value>, Error> {
        let doctype = rb_self.with_arena(ruby, |arena, _| arena.doctype())?;
        doctype
            .map(|id| wrap(ruby, rb_self.document(ruby)?, id))
            .transpose()
    }
}

fn uninitialized(ruby: &Ruby) -> Error {
    let class = ruby.get_inner(&NODE);
    let message = format!(
        "Uninitialized {} struct (null data pointer)",
        class.inspect()
    );
    Error::new(ruby.exception_runtime_error(), message)
}

/// The one Ruby object for a node, created on first use: Nokogiri's
/// `noko_xml_node_wrap`. The class follows the node type unless `class` is
/// given. A new object goes into the document's `@node_cache` and through
/// `Document#decorate`; `initialize` is not called.
pub fn wrap_as(
    ruby: &Ruby,
    document: Value,
    id: NodeId,
    class: Option<RClass>,
) -> Result<Value, Error> {
    if id == NodeId::DOCUMENT {
        return Ok(document);
    }
    let cache: RArray = Obj::<Node>::try_convert(document)?.ivar_get("@node_cache")?;
    let cached: Value = cache.entry(cache_index(id))?;
    if !cached.is_nil() {
        return Ok(cached);
    }
    let class = match class {
        Some(class) => class,
        None => {
            let node_type = document::with_arena(ruby, document, |arena| arena.node_type(id))?;
            class_for(ruby, node_type)
        }
    };
    let node = ruby.obj_wrap_as(Node::new(document, id), class).as_value();
    cache.store(cache_index(id), node)?;
    document.funcall::<_, _, Value>("decorate", (node,))?;
    Ok(node)
}

pub fn wrap(ruby: &Ruby, document: Value, id: NodeId) -> Result<Value, Error> {
    wrap_as(ruby, document, id, None)
}

fn cache_index(id: NodeId) -> isize {
    isize::try_from(id.index()).expect("node ids fit in an isize")
}

/// Nokogiri's table from node type to class, for the types an HTML tree has.
fn class_for(ruby: &Ruby, node_type: NodeType) -> RClass {
    match node_type {
        NodeType::Element => ruby.get_inner(&ELEMENT),
        NodeType::Text => ruby.get_inner(&TEXT),
        NodeType::CDataSection => ruby.get_inner(&CDATA),
        NodeType::Comment => ruby.get_inner(&COMMENT),
        NodeType::DocumentFragment => ruby.get_inner(&DOCUMENT_FRAGMENT),
        NodeType::Dtd => ruby.get_inner(&DTD),
        NodeType::Attribute => ruby.get_inner(&ATTR),
        NodeType::ProcessingInstruction | NodeType::Document | NodeType::HtmlDocument => {
            ruby.get_inner(&NODE)
        }
    }
}

/// The document a new node goes into. Passing another node instead is
/// deprecated in Nokogiri and resolves to that node's document.
pub fn document_for_new_node(
    ruby: &Ruby,
    document: Value,
    deprecation: &str,
) -> Result<Value, Error> {
    if document.is_kind_of(ruby.get_inner(&document::DOCUMENT)) {
        return Ok(document);
    }
    warn_deprecated(ruby, deprecation)?;
    <&Node>::try_convert(document)?.document(ruby)
}

/// Shared tail of Nokogiri's native constructors: wrap as the receiving
/// class, run `initialize` with the original arguments, then yield.
pub fn finish_new(
    ruby: &Ruby,
    document: Value,
    id: NodeId,
    class: RClass,
    args: &[Value],
) -> Result<Value, Error> {
    let node = wrap_as(ruby, document, id, Some(class))?;
    call_initialize(node, args)?;
    yield_to_block(ruby, node)?;
    Ok(node)
}

/// `Node.new(name, document, *rest)`.
fn new(ruby: &Ruby, class: RClass, args: &[Value]) -> Result<Value, Error> {
    ruby.check_arity(args.len(), 2..)?;
    let node_class = ruby.get_inner(&NODE);
    if !args[1].is_kind_of(node_class) {
        let message = format!("document must be a {}", node_class.inspect());
        return Err(Error::new(ruby.exception_arg_error(), message));
    }
    let document = document_for_new_node(
        ruby,
        args[1],
        "Passing a Node as the second parameter to Node.new is deprecated. Please pass a Document instead, \
         or prefer an alternative constructor like Node#add_child.",
    )?;
    let name = RString::try_convert(args[0])?.to_string()?;
    let id = document::with_arena_mut(ruby, document, |arena| arena.create_element_named(&name))?;
    // `Node.new` gives an Element, as libxml2's node type decides
    let class = if class.equal(node_class)? {
        ruby.get_inner(&ELEMENT)
    } else {
        class
    };
    finish_new(ruby, document, id, class, args)
}

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let class = ruby.get_inner(&NODE);
    class.define_alloc_func::<Node>();
    class.define_singleton_method("new", method!(new, -1))?;
    class.define_method("data_ptr?", method!(Node::data_ptr_p, 0))?;
    class.define_method("document", method!(Node::document_method, 0))?;
    class.define_method("node_name", method!(Node::node_name, 0))?;
    class.define_method("node_type", method!(Node::node_type, 0))?;
    class.define_method("internal_subset", method!(Node::internal_subset, 0))?;
    Ok(())
}
