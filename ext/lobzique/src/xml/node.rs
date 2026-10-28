//! `XML::Node`, and the wrapper every node object shares, documents included.

use super::ELEMENT;
use super::{
    attr::ATTR, cdata::CDATA, comment::COMMENT, document, document_fragment::DOCUMENT_FRAGMENT,
    dtd::DTD, namespace, text::TEXT,
};
use crate::support::{c_string, call_initialize, obj_class, warn_deprecated, yield_to_block};
use crate::XML;
use lobzique_core::dom::{
    self, CopyDepth, Doctype, NamespaceRef, NodeId, NodeType, Position, ReparentError,
};
use lobzique_core::html5;
use magnus::{
    gc, method,
    prelude::*,
    rb_sys::AsRawValue,
    typed_data::Obj,
    value::{Lazy, Opaque},
    DataTypeFunctions, Error, RArray, RClass, RString, Ruby, TypedData, Value,
};
use std::cell::{Cell, OnceCell, RefCell};
use std::mem::size_of;

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

    /// A document reports its arena, which holds all of its nodes.
    fn size(&self) -> usize {
        let arena = self.arena.get().and_then(|arena| arena.try_borrow().ok());
        size_of::<Self>() + arena.map_or(0, |arena| arena.heap_size())
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

    /// Gives an allocated, empty object the arena it is the document of.
    pub fn adopt_arena(&self, arena: dom::Document) -> Result<(), RefCell<dom::Document>> {
        self.arena.set(RefCell::new(arena))?;
        self.id.set(Some(NodeId::DOCUMENT));
        Ok(())
    }

    /// Points the object at another node, possibly in another document, as
    /// Nokogiri resets `DATA_PTR` after copying or merging a node.
    fn repoint(&self, document: Value, id: NodeId) {
        self.document.set(Some(document.into()));
        self.id.set(Some(id));
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

    /// Runs `f` on the node's arena, under the rules of
    /// [`document::with_arena`] (plan.md decision 5).
    pub fn with_arena<R: Send>(
        &self,
        ruby: &Ruby,
        f: impl FnOnce(&dom::Document, NodeId) -> R + Send,
    ) -> Result<R, Error> {
        let id = self.id(ruby)?;
        document::with_arena(ruby, self.document(ruby)?, |arena| f(arena, id))
    }

    fn node_type(ruby: &Ruby, rb_self: &Self) -> Result<u8, Error> {
        rb_self.with_arena(ruby, |arena, id| arena.node_type(id) as u8)
    }

    fn node_name(ruby: &Ruby, rb_self: &Self) -> Result<Option<RString>, Error> {
        let name = rb_self.with_arena(ruby, |arena, id| arena.node_name(id).map(<[u8]>::to_vec))?;
        Ok(name.map(|name| utf8_string(ruby, name)))
    }

    fn data_ptr_p(&self) -> bool {
        self.document.get().is_some()
    }

    fn document_method(ruby: &Ruby, rb_self: &Self) -> Result<Value, Error> {
        rb_self.document(ruby)
    }

    /// The node object for the node `f` finds from this one, if any.
    fn wrap_found(
        &self,
        ruby: &Ruby,
        f: impl FnOnce(&dom::Document, NodeId) -> Option<NodeId> + Send,
    ) -> Result<Option<Value>, Error> {
        self.with_arena(ruby, f)?
            .map(|id| wrap(ruby, self.document(ruby)?, id))
            .transpose()
    }

    fn internal_subset(ruby: &Ruby, rb_self: &Self) -> Result<Option<Value>, Error> {
        rb_self.wrap_found(ruby, |arena, _| arena.doctype())
    }

    fn parent(ruby: &Ruby, rb_self: &Self) -> Result<Option<Value>, Error> {
        rb_self.wrap_found(ruby, |arena, id| arena.parent(id))
    }

    fn child(ruby: &Ruby, rb_self: &Self) -> Result<Option<Value>, Error> {
        rb_self.wrap_found(ruby, |arena, id| arena.first_child(id))
    }

    fn next_sibling(ruby: &Ruby, rb_self: &Self) -> Result<Option<Value>, Error> {
        rb_self.wrap_found(ruby, |arena, id| arena.next_in_parent(id))
    }

    fn previous_sibling(ruby: &Ruby, rb_self: &Self) -> Result<Option<Value>, Error> {
        rb_self.wrap_found(ruby, |arena, id| arena.previous_in_parent(id))
    }

    fn next_element(ruby: &Ruby, rb_self: &Self) -> Result<Option<Value>, Error> {
        rb_self.wrap_found(ruby, |arena, id| arena.next_element_sibling(id))
    }

    fn previous_element(ruby: &Ruby, rb_self: &Self) -> Result<Option<Value>, Error> {
        rb_self.wrap_found(ruby, |arena, id| arena.previous_element_sibling(id))
    }

    fn first_element_child(ruby: &Ruby, rb_self: &Self) -> Result<Option<Value>, Error> {
        rb_self.wrap_found(ruby, |arena, id| arena.element_children(id).next())
    }

    fn last_element_child(ruby: &Ruby, rb_self: &Self) -> Result<Option<Value>, Error> {
        rb_self.wrap_found(ruby, |arena, id| arena.last_element_child(id))
    }

    fn children(ruby: &Ruby, rb_self: &Self) -> Result<Value, Error> {
        let ids = rb_self.with_arena(ruby, |arena, id| arena.children(id).collect())?;
        node_set(ruby, rb_self.document(ruby)?, ids)
    }

    fn element_children(ruby: &Ruby, rb_self: &Self) -> Result<Value, Error> {
        let ids = rb_self.with_arena(ruby, |arena, id| arena.element_children(id).collect())?;
        node_set(ruby, rb_self.document(ruby)?, ids)
    }

    fn attribute(ruby: &Ruby, rb_self: &Self, name: Value) -> Result<Option<Value>, Error> {
        let name = c_string(name)?;
        rb_self.wrap_found(ruby, move |arena, id| arena.attribute_named(id, &name))
    }

    fn attribute_with_ns(
        ruby: &Ruby,
        rb_self: &Self,
        name: Value,
        namespace: Value,
    ) -> Result<Option<Value>, Error> {
        let (name, href) = (c_string(name)?, optional_c_string(namespace)?);
        rb_self.wrap_found(ruby, move |arena, id| {
            arena.attribute_in_namespace(id, &name, href.as_deref())
        })
    }

    fn key_p(ruby: &Ruby, rb_self: &Self, name: Value) -> Result<bool, Error> {
        let name = c_string(name)?;
        rb_self.with_arena(ruby, move |arena, id| {
            arena.attribute_named(id, &name).is_some()
        })
    }

    fn namespaced_key_p(
        ruby: &Ruby,
        rb_self: &Self,
        name: Value,
        namespace: Value,
    ) -> Result<bool, Error> {
        let (name, href) = (c_string(name)?, optional_c_string(namespace)?);
        rb_self.with_arena(ruby, move |arena, id| {
            arena
                .attribute_in_namespace(id, &name, href.as_deref())
                .is_some()
        })
    }

    /// `Node#[]`'s native half.
    fn get(ruby: &Ruby, rb_self: &Self, name: Value) -> Result<Option<RString>, Error> {
        let Some(name) = optional_c_string(name)? else {
            return Ok(None);
        };
        let value = rb_self.with_arena(ruby, move |arena, id| {
            arena.attribute_value(id, &name).map(<[u8]>::to_vec)
        })?;
        Ok(value.map(|value| utf8_string(ruby, value)))
    }

    fn lang(ruby: &Ruby, rb_self: &Self) -> Result<Option<RString>, Error> {
        let lang = rb_self.with_arena(ruby, |arena, id| arena.lang(id).map(<[u8]>::to_vec))?;
        Ok(lang.map(|lang| utf8_string(ruby, lang)))
    }

    fn blank_p(ruby: &Ruby, rb_self: &Self) -> Result<bool, Error> {
        rb_self.with_arena(ruby, |arena, id| arena.is_blank(id))
    }

    fn path(ruby: &Ruby, rb_self: &Self) -> Result<RString, Error> {
        let path = rb_self.with_arena(ruby, |arena, id| arena.path(id))?;
        Ok(utf8_string(ruby, path.as_deref().unwrap_or(b"?")))
    }

    /// Unique among live nodes, as libxml2's node address is: the address
    /// of the document's data, which doesn't move, and the node's id.
    fn pointer_id(ruby: &Ruby, rb_self: &Self) -> Result<u128, Error> {
        let document = document::unwrap(ruby, rb_self.document(ruby)?)?;
        let address = document as *const Node as usize;
        Ok((address as u128) << 32 | rb_self.id(ruby)?.index() as u128)
    }

    /// `xmlXPathCmpNodes(other, self)`: 1 if this node comes after `other`,
    /// -1 if before, 0 if they are the same, -2 if they are in different
    /// trees.
    fn compare(ruby: &Ruby, rb_self: &Self, other: Value) -> Result<i8, Error> {
        let other = <&Node>::try_convert(other)?;
        let document = rb_self.document(ruby)?;
        if document.as_raw() != other.document(ruby)?.as_raw() {
            return Ok(-2);
        }
        let other = other.id(ruby)?;
        let order = rb_self.with_arena(ruby, move |arena, id| arena.document_order(id, other))?;
        Ok(order.map_or(-2, |order| order as i8))
    }

    fn namespace_definitions(ruby: &Ruby, rb_self: &Self) -> Result<RArray, Error> {
        let definitions = rb_self.with_arena(ruby, |arena, id| {
            owned_namespaces(arena.namespace_definitions(id).iter().map(Into::into))
        })?;
        wrap_namespaces(ruby, rb_self.document(ruby)?, definitions)
    }

    fn namespace_scopes(ruby: &Ruby, rb_self: &Self) -> Result<RArray, Error> {
        let scopes = rb_self.with_arena(ruby, |arena, id| {
            owned_namespaces(arena.namespace_scopes(id).into_iter())
        })?;
        wrap_namespaces(ruby, rb_self.document(ruby)?, scopes)
    }

    fn encode_special_chars(ruby: &Ruby, _rb_self: &Self, string: Value) -> Result<RString, Error> {
        Ok(utf8_string(
            ruby,
            dom::encode_special_chars(&c_string(string)?),
        ))
    }

    fn attribute_nodes(ruby: &Ruby, rb_self: &Self) -> Result<RArray, Error> {
        let ids = rb_self.with_arena(ruby, |arena, id| arena.attributes(id).to_vec())?;
        wrap_all(ruby, rb_self.document(ruby)?, ids)
    }

    fn content(ruby: &Ruby, rb_self: &Self) -> Result<Option<RString>, Error> {
        let content = rb_self.with_arena(ruby, |arena, id| arena.content(id))?;
        Ok(content.map(|content| utf8_string(ruby, content)))
    }

    fn namespace(ruby: &Ruby, rb_self: &Self) -> Result<Option<Value>, Error> {
        let namespace = rb_self.with_arena(ruby, |arena, id| {
            arena
                .namespace(id)
                .map(|ns| (ns.prefix.map(str::to_owned), ns.href.to_owned()))
        })?;
        namespace
            .map(|(prefix, href)| namespace::wrap(ruby, rb_self.document(ruby)?, prefix, href))
            .transpose()
    }

    fn html_standard_serialize(
        ruby: &Ruby,
        rb_self: &Self,
        preserve_newline: Value,
    ) -> Result<RString, Error> {
        let preserve_newline = preserve_newline.to_bool();
        let html = rb_self.with_arena(ruby, |arena, id| {
            html5::serialize(arena, id, preserve_newline)
        })?;
        Ok(utf8_string(ruby, html))
    }

    fn prepend_newline_p(ruby: &Ruby, rb_self: &Self) -> Result<bool, Error> {
        rb_self.with_arena(ruby, html5::prepend_newline)
    }

    fn add_child_node(ruby: &Ruby, rb_self: &Self, node: Value) -> Result<Value, Error> {
        reparent(ruby, rb_self, node, Position::LastChild)
    }

    fn add_previous_sibling_node(ruby: &Ruby, rb_self: &Self, node: Value) -> Result<Value, Error> {
        reparent(ruby, rb_self, node, Position::Before)
    }

    fn add_next_sibling_node(ruby: &Ruby, rb_self: &Self, node: Value) -> Result<Value, Error> {
        reparent(ruby, rb_self, node, Position::After)
    }

    fn replace_node(ruby: &Ruby, rb_self: &Self, node: Value) -> Result<Value, Error> {
        reparent(ruby, rb_self, node, Position::Replace)
    }

    fn unlink(ruby: &Ruby, rb_self: Obj<Self>) -> Result<Obj<Self>, Error> {
        rb_self.with_arena_mut(ruby, |arena, id| arena.detach(id))?;
        Ok(rb_self)
    }

    /// `Node#[]=`'s native half: `nil`, and no change, unless an element.
    fn set(ruby: &Ruby, rb_self: &Self, name: Value, value: Value) -> Result<Value, Error> {
        let is_element =
            rb_self.with_arena(ruby, |arena, id| arena.node_type(id) == NodeType::Element)?;
        if !is_element {
            return Ok(ruby.qnil().as_value());
        }
        let name = element_name(ruby, c_string(name)?)?;
        let bytes = c_string(value)?;
        rb_self.with_arena_mut(ruby, move |arena, id| {
            arena.set_attribute(id, &name, bytes);
        })?;
        Ok(value)
    }

    fn set_native_content(ruby: &Ruby, rb_self: &Self, content: Value) -> Result<Value, Error> {
        let bytes = c_string(content)?;
        rb_self.with_arena_mut(ruby, move |arena, id| arena.set_content(id, bytes))?;
        Ok(content)
    }

    fn set_name(ruby: &Ruby, rb_self: &Self, new_name: Value) -> Result<Value, Error> {
        let name = element_name(ruby, c_string(new_name)?)?;
        rb_self.with_arena_mut(ruby, move |arena, id| arena.set_name(id, &name))?;
        Ok(new_name)
    }

    fn set_lang(ruby: &Ruby, rb_self: &Self, lang: Value) -> Result<(), Error> {
        let lang = c_string(lang)?;
        rb_self.with_arena_mut(ruby, move |arena, id| arena.set_lang(id, lang))
    }

    fn create_internal_subset(
        ruby: &Ruby,
        rb_self: &Self,
        name: Value,
        external_id: Value,
        system_id: Value,
    ) -> Result<Value, Error> {
        let doctype = Doctype {
            name: optional_c_string(name)?.unwrap_or_default(),
            public_id: optional_c_string(external_id)?,
            system_id: optional_c_string(system_id)?,
        };
        let document = rb_self.document(ruby)?;
        let dtd = document::with_arena_mut(ruby, document, |arena| {
            arena.create_internal_subset(doctype)
        })?
        .ok_or_else(|| {
            Error::new(
                ruby.exception_runtime_error(),
                "Document already has an internal subset",
            )
        })?;
        wrap(ruby, document, dtd)
    }

    /// `initialize_copy_with_args(other, level, new_parent_doc)`, after
    /// `Object#dup` or `#clone` has allocated the copy: `xmlDocCopyNode`,
    /// then the object is cached and decorated. `nil` for nodes libxml2
    /// doesn't copy, such as doctypes.
    fn initialize_copy_with_args(
        ruby: &Ruby,
        rb_self: Obj<Self>,
        other: Value,
        level: i32,
        new_parent_doc: Value,
    ) -> Result<Option<Obj<Self>>, Error> {
        let node_class = ruby.get_inner(&NODE);
        if !other.is_kind_of(node_class) {
            let message = format!("argument must be a kind of {}", node_class.inspect());
            return Err(Error::new(ruby.exception_type_error(), message));
        }
        // `Object#dup` hands over a fresh object; repointing a live one
        // would leave it in its old node's cache slot
        if rb_self.data_ptr_p() {
            return Err(Error::new(
                ruby.exception_runtime_error(),
                "node is already initialized",
            ));
        }
        let other = <&Node>::try_convert(other)?;
        let (source, id) = (other.document(ruby)?, other.id(ruby)?);
        document::unwrap(ruby, new_parent_doc)?;
        let depth = CopyDepth::from_level(level);
        let copy = if source.as_raw() == new_parent_doc.as_raw() {
            document::with_arena_mut(ruby, source, move |arena| arena.copy(id, depth))?
        } else {
            document::with_arenas_mut(ruby, new_parent_doc, source, move |arena, source| {
                arena.import(source, id, depth)
            })?
        };
        let Some(copy) = copy else {
            return Ok(None);
        };
        rb_self.repoint(new_parent_doc, copy);
        let cache: RArray = Obj::<Node>::try_convert(new_parent_doc)?.ivar_get("@node_cache")?;
        cache.store(cache_index(copy), rb_self)?;
        new_parent_doc.funcall::<_, _, Value>("decorate", (rb_self,))?;
        Ok(Some(rb_self))
    }

    /// [`Node::with_arena`] with a mutable borrow.
    pub fn with_arena_mut<R: Send>(
        &self,
        ruby: &Ruby,
        f: impl FnOnce(&mut dom::Document, NodeId) -> R + Send,
    ) -> Result<R, Error> {
        let id = self.id(ruby)?;
        document::with_arena_mut(ruby, self.document(ruby)?, |arena| f(arena, id))
    }
}

/// Nokogiri's `reparent_node_with`. A node that the core copied, because
/// it is text or from another document, or merged into neighboring text,
/// keeps its object, which is pointed at what went into the tree; that
/// node gets an object of its own, which is decorated and returned. As in
/// Nokogiri, a text node's original loses its cached object.
fn reparent(ruby: &Ruby, pivot: &Node, node: Value, position: Position) -> Result<Value, Error> {
    let node_class = ruby.get_inner(&NODE);
    if !node.is_kind_of(node_class) || node.is_kind_of(ruby.get_inner(&document::DOCUMENT)) {
        let message = format!("node must be a {}", node_class.inspect());
        return Err(Error::new(ruby.exception_arg_error(), message));
    }
    let reparentee = <&Node>::try_convert(node)?;
    let (document, pivot_id) = (pivot.document(ruby)?, pivot.id(ruby)?);
    let (source, id) = (reparentee.document(ruby)?, reparentee.id(ruby)?);
    let namespace_inheritance = Obj::<Node>::try_convert(document)?
        .ivar_get::<_, Value>("@namespace_inheritance")?
        .to_bool();
    let is_text = reparentee.with_arena(ruby, |arena, id| arena.node_type(id) == NodeType::Text)?;

    let reparented = if source.as_raw() == document.as_raw() {
        document::with_arena_mut(ruby, document, |arena| {
            arena.reparent(pivot_id, position, id, namespace_inheritance)
        })?
    } else {
        document::with_arenas_mut(ruby, document, source, |arena, source| {
            arena.reparent_from(pivot_id, position, source, id, namespace_inheritance)
        })?
    };
    let reparented = match reparented {
        Ok(reparented) => reparented,
        Err(ReparentError::NotAllowed) => {
            let message = format!("cannot reparent {} there", obj_class(node).inspect());
            return Err(Error::new(ruby.exception_arg_error(), message));
        }
        Err(ReparentError::Refused) => {
            return Err(Error::new(
                ruby.exception_runtime_error(),
                "Could not reparent node",
            ));
        }
        Err(ReparentError::Cycle) => {
            let name = reparentee.with_arena(ruby, |arena, id| {
                String::from_utf8_lossy(arena.node_name(id).unwrap_or_default()).into_owned()
            })?;
            let message = format!("cycle detected: node '{name}' is an ancestor of itself");
            return Err(Error::new(ruby.exception_runtime_error(), message));
        }
    };

    if is_text {
        let cache: RArray = Obj::<Node>::try_convert(source)?.ivar_get("@node_cache")?;
        cache.store(cache_index(id), ruby.qnil())?;
    }
    reparentee.repoint(document, reparented);
    let reparented = wrap(ruby, document, reparented)?;
    reparented.funcall::<_, _, Value>("decorate!", ())?;
    Ok(reparented)
}

/// A string argument as [`c_string`] reads it, or `None` for `nil`.
fn optional_c_string(value: Value) -> Result<Option<Vec<u8>>, Error> {
    if value.is_nil() {
        Ok(None)
    } else {
        c_string(value).map(Some)
    }
}

type OwnedNamespace = (Option<String>, String);

fn owned_namespaces<'a>(namespaces: impl Iterator<Item = NamespaceRef<'a>>) -> Vec<OwnedNamespace> {
    namespaces
        .map(|namespace| {
            (
                namespace.prefix.map(str::to_owned),
                namespace.href.to_owned(),
            )
        })
        .collect()
}

fn wrap_namespaces(
    ruby: &Ruby,
    document: Value,
    namespaces: Vec<OwnedNamespace>,
) -> Result<RArray, Error> {
    let array = ruby.ary_new_capa(namespaces.len());
    for (prefix, href) in namespaces {
        array.push(namespace::wrap(ruby, document, prefix, href)?)?;
    }
    Ok(array)
}

/// The node objects for `ids`, in an Array.
pub fn wrap_all(ruby: &Ruby, document: Value, ids: Vec<NodeId>) -> Result<RArray, Error> {
    let nodes = ruby.ary_new_capa(ids.len());
    for id in ids {
        nodes.push(wrap(ruby, document, id)?)?;
    }
    Ok(nodes)
}

static NODE_SET: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .const_get("NodeSet")
        .expect("lib/lobzique/xml/node_set.rb defines XML::NodeSet")
});

/// A NodeSet of the node objects for `ids`: `noko_xml_node_set_wrap`, which
/// is Ruby's `NodeSet.wrap` here (ARCHITECTURE.md, departures).
pub fn node_set(ruby: &Ruby, document: Value, ids: Vec<NodeId>) -> Result<Value, Error> {
    let nodes = wrap_all(ruby, document, ids)?;
    ruby.get_inner(&NODE_SET).funcall("wrap", (document, nodes))
}

/// Bytes from the arena as a Ruby string tagged UTF-8, valid or not, as
/// Nokogiri returns what libxml2 holds.
pub fn utf8_string(ruby: &Ruby, bytes: impl AsRef<[u8]>) -> RString {
    ruby.enc_str_new(bytes, ruby.utf8_encoding())
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
    let name = element_name(ruby, c_string(args[0])?)?;
    let id = document::with_arena_mut(ruby, document, |arena| arena.create_element_named(&name))?;
    // `Node.new` gives an Element, as libxml2's node type decides
    let class = if class.equal(node_class)? {
        ruby.get_inner(&ELEMENT)
    } else {
        class
    };
    finish_new(ruby, document, id, class, args)
}

/// Names are atoms shared with the parser and selector engine, so unlike
/// Nokogiri's they must be UTF-8 (ARCHITECTURE.md, departures).
fn element_name(ruby: &Ruby, name: Vec<u8>) -> Result<String, Error> {
    String::from_utf8(name)
        .map_err(|_| Error::new(ruby.exception_arg_error(), "invalid byte sequence in UTF-8"))
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
    class.define_method("parent", method!(Node::parent, 0))?;
    class.define_method("child", method!(Node::child, 0))?;
    class.define_method("children", method!(Node::children, 0))?;
    class.define_method("element_children", method!(Node::element_children, 0))?;
    class.define_method("first_element_child", method!(Node::first_element_child, 0))?;
    class.define_method("last_element_child", method!(Node::last_element_child, 0))?;
    class.define_method("next_sibling", method!(Node::next_sibling, 0))?;
    class.define_method("previous_sibling", method!(Node::previous_sibling, 0))?;
    class.define_method("next_element", method!(Node::next_element, 0))?;
    class.define_method("previous_element", method!(Node::previous_element, 0))?;
    class.define_method("attribute", method!(Node::attribute, 1))?;
    class.define_method("attribute_with_ns", method!(Node::attribute_with_ns, 2))?;
    class.define_method("key?", method!(Node::key_p, 1))?;
    class.define_method("namespaced_key?", method!(Node::namespaced_key_p, 2))?;
    class.define_method("blank?", method!(Node::blank_p, 0))?;
    class.define_method("lang", method!(Node::lang, 0))?;
    class.define_method("path", method!(Node::path, 0))?;
    class.define_method("pointer_id", method!(Node::pointer_id, 0))?;
    class.define_method(
        "encode_special_chars",
        method!(Node::encode_special_chars, 1),
    )?;
    class.define_method("attribute_nodes", method!(Node::attribute_nodes, 0))?;
    class.define_method("content", method!(Node::content, 0))?;
    class.define_method("namespace", method!(Node::namespace, 0))?;
    class.define_method(
        "namespace_definitions",
        method!(Node::namespace_definitions, 0),
    )?;
    class.define_method("namespace_scopes", method!(Node::namespace_scopes, 0))?;
    class.define_method("unlink", method!(Node::unlink, 0))?;
    class.define_method("native_content=", method!(Node::set_native_content, 1))?;
    class.define_method("node_name=", method!(Node::set_name, 1))?;
    class.define_method("lang=", method!(Node::set_lang, 1))?;
    class.define_method(
        "create_internal_subset",
        method!(Node::create_internal_subset, 3),
    )?;
    class.define_protected_method(
        "initialize_copy_with_args",
        method!(Node::initialize_copy_with_args, 3),
    )?;
    class.define_private_method("add_child_node", method!(Node::add_child_node, 1))?;
    class.define_private_method(
        "add_previous_sibling_node",
        method!(Node::add_previous_sibling_node, 1),
    )?;
    class.define_private_method(
        "add_next_sibling_node",
        method!(Node::add_next_sibling_node, 1),
    )?;
    class.define_private_method("replace_node", method!(Node::replace_node, 1))?;
    class.define_private_method("set", method!(Node::set, 2))?;
    class.define_private_method("compare", method!(Node::compare, 1))?;
    class.define_private_method("get", method!(Node::get, 1))?;
    class.define_private_method("prepend_newline?", method!(Node::prepend_newline_p, 0))?;
    class.define_private_method(
        "html_standard_serialize",
        method!(Node::html_standard_serialize, 1),
    )?;
    Ok(())
}
