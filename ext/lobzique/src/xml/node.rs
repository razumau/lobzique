//! `XML::Node` and the wrapper that all node objects share, documents included.

use super::ELEMENT;
use super::{
    attr::ATTR, cdata::CDATA, comment::COMMENT, document, document_fragment::DOCUMENT_FRAGMENT,
    dtd::DTD, namespace, node_set, text::TEXT,
};
use crate::html5ever;
use crate::support::{c_string, call_initialize, obj_class, warn_deprecated, yield_to_block};
use crate::XML;
use lobzique_core::dom::{
    self, CopyDepth, Doctype, NamespaceRef, NodeId, NodeType, Position, ReparentError,
};
use lobzique_core::{html5, xml};
use magnus::{
    block::Proc,
    encoding::{Encoding, EncodingCapable, RbEncoding},
    gc, method,
    prelude::*,
    rb_sys::AsRawValue,
    typed_data::{Obj, Writebarrier},
    value::{Lazy, Opaque},
    DataTypeFunctions, Error, KwArgs, RArray, RClass, RString, Ruby, TypedData, Value,
};
use std::cell::{Cell, OnceCell, RefCell};
use std::mem::size_of;

pub static NODE: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .define_class("Node", ruby.class_object())
        .unwrap()
});

/// The data behind every node object. A node is its document plus its id
/// in the arena of that document. Both change when the node moves to another
/// document. A document is the node that owns the arena, so `Document < Node`
/// works without a second data type.
///
/// Its objects are write-barrier protected, so that a minor GC does not mark
/// every node object of every live document. Only `complete_document`
/// and `repoint` store the two Ruby references, and they tell the GC.
#[derive(Default, TypedData)]
#[magnus(
    class = "Lobzique::XML::Node",
    mark,
    compact,
    size,
    free_immediately,
    wb_protected
)]
pub struct Node {
    /// `None` only on an object made with `allocate` and never initialized.
    /// Typed, so that reaching the arena from a node needs no check.
    document: Cell<Option<Opaque<Obj<Node>>>>,
    id: Cell<Option<NodeId>>,
    /// Boxed, so that the objects of other nodes do not carry an empty one.
    owned: OnceCell<Box<Owned>>,
}

struct Owned {
    arena: RefCell<dom::Document>,
    /// Node objects by node id: the `@node_cache` of Nokogiri. `None`
    /// until the own object of the document exists.
    cache: Cell<Option<Opaque<RArray>>>,
}

impl Owned {
    fn new(arena: dom::Document) -> Box<Owned> {
        Box::new(Owned {
            arena: RefCell::new(arena),
            cache: Cell::new(None),
        })
    }
}

impl DataTypeFunctions for Node {
    fn mark(&self, marker: &gc::Marker) {
        if let Some(document) = self.document.get() {
            marker.mark_movable(document);
        }
        if let Some(cache) = self.owned.get().and_then(|owned| owned.cache.get()) {
            marker.mark(cache);
        }
    }

    fn compact(&self, compactor: &gc::Compactor) {
        if let Some(document) = self.document.get() {
            self.document.set(Some(compactor.location(document)));
        }
    }

    fn size(&self) -> usize {
        let Some(owned) = self.owned.get() else {
            return size_of::<Self>();
        };
        let heap = owned
            .arena
            .try_borrow()
            .map_or(0, |arena| arena.heap_size());
        size_of::<Self>() + size_of::<Owned>() + heap
    }
}

impl Node {
    fn new(document: Obj<Node>, id: NodeId) -> Node {
        Node {
            document: Cell::new(Some(document.into())),
            id: Cell::new(Some(id)),
            owned: OnceCell::new(),
        }
    }

    /// The data of the document node itself, which owns the arena.
    /// It is not usable before [`Node::complete_document`].
    pub fn new_document(arena: dom::Document) -> Node {
        Node {
            document: Cell::new(None),
            id: Cell::new(Some(NodeId::DOCUMENT)),
            owned: OnceCell::from(Owned::new(arena)),
        }
    }

    /// Gives an allocated, empty object the arena that it is the document
    /// of. `false` if it has one already.
    pub fn adopt_arena(&self, arena: dom::Document) -> bool {
        if self.owned.set(Owned::new(arena)).is_err() {
            return false;
        }
        self.id.set(Some(NodeId::DOCUMENT));
        true
    }

    /// Called once the Ruby object of a document exists. It is its own document
    /// and starts with an empty node cache.
    pub fn complete_document(ruby: &Ruby, document: Obj<Node>) {
        document.document.set(Some(document.into()));
        if let Some(owned) = document.owned.get() {
            let cache = ruby.ary_new();
            owned.cache.set(Some(cache.into()));
            document.writebarrier(cache);
        }
    }

    /// Points the object at another node, possibly in another document,
    /// as Nokogiri resets `DATA_PTR` after it copies or merges a node.
    fn repoint(node: Obj<Node>, document: Obj<Node>, id: NodeId) {
        node.document.set(Some(document.into()));
        node.id.set(Some(id));
        node.writebarrier(document);
    }

    fn owned(&self, ruby: &Ruby) -> Result<&Owned, Error> {
        self.owned.get().map(Box::as_ref).ok_or_else(|| {
            Error::new(
                ruby.exception_runtime_error(),
                "Uninitialized document struct (null data pointer)",
            )
        })
    }

    pub fn arena(&self, ruby: &Ruby) -> Result<&RefCell<dom::Document>, Error> {
        Ok(&self.owned(ruby)?.arena)
    }

    fn cache(&self, ruby: &Ruby) -> Result<RArray, Error> {
        let cache = self.owned(ruby)?.cache.get();
        Ok(ruby.get_inner(cache.expect("a document object has a node cache")))
    }

    pub fn document(&self, ruby: &Ruby) -> Result<Obj<Node>, Error> {
        self.document
            .get()
            .map(|document| ruby.get_inner(document))
            .ok_or_else(|| uninitialized(ruby))
    }

    pub fn id(&self, ruby: &Ruby) -> Result<NodeId, Error> {
        self.id.get().ok_or_else(|| uninitialized(ruby))
    }

    /// Runs `f` on the arena of the node, under the rules
    /// of [`document::with_arena`] (plan.md decision 5).
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

    fn document_method(ruby: &Ruby, rb_self: &Self) -> Result<Obj<Node>, Error> {
        rb_self.document(ruby)
    }

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
        node_set::wrap(ruby, rb_self.document(ruby)?, ids)
    }

    fn element_children(ruby: &Ruby, rb_self: &Self) -> Result<Value, Error> {
        let ids = rb_self.with_arena(ruby, |arena, id| arena.element_children(id).collect())?;
        node_set::wrap(ruby, rb_self.document(ruby)?, ids)
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

    /// Unique among live nodes, as the address of a libxml2 node
    /// is: the address of the data of the document, which does not move,
    /// and the id of the node.
    fn pointer_id(ruby: &Ruby, rb_self: &Self) -> Result<u128, Error> {
        let document = rb_self.document(ruby)?;
        let address = &*document as *const Node as usize;
        Ok((address as u128) << 32 | rb_self.id(ruby)?.index() as u128)
    }

    /// `xmlXPathCmpNodes(other, self)`: 1 if this node comes after `other`, -1
    /// if before, 0 if they are the same, -2 if they are in different trees.
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
        wrap_namespaces(ruby, rb_self.document(ruby)?.as_value(), definitions)
    }

    fn namespace_scopes(ruby: &Ruby, rb_self: &Self) -> Result<RArray, Error> {
        let scopes = rb_self.with_arena(ruby, |arena, id| {
            owned_namespaces(arena.namespace_scopes(id).into_iter())
        })?;
        wrap_namespaces(ruby, rb_self.document(ruby)?.as_value(), scopes)
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
        let document = rb_self.document(ruby)?.as_value();
        namespace
            .map(|(prefix, href)| namespace::wrap(ruby, document, prefix, href))
            .transpose()
    }

    fn html_standard_serialize(
        ruby: &Ruby,
        rb_self: &Self,
        preserve_newline: Value,
        scripting_enabled: Value,
    ) -> Result<RString, Error> {
        let preserve_newline = preserve_newline.to_bool();
        let scripting_enabled = scripting_enabled.to_bool();
        let html = rb_self.with_arena(ruby, |arena, id| {
            html5::serialize(arena, id, preserve_newline, scripting_enabled)
        })?;
        Ok(utf8_string(ruby, html))
    }

    /// The XML and XHTML output of libxml2 for the node, transcoded and written
    /// to `io`.
    fn native_write_to(
        ruby: &Ruby,
        rb_self: &Self,
        io: Value,
        encoding: Value,
        indent_string: Value,
        options: i32,
    ) -> Result<Value, Error> {
        let encoding = optional_c_string(encoding)?;
        let indent = c_string(indent_string)?;
        let output = rb_self
            .with_arena(ruby, |arena, id| {
                xml::write(arena, id, encoding.as_deref(), &indent, options as u32)
            })?
            .map_err(|xml::HtmlUnsupported| {
                Error::new(
                    ruby.exception_not_imp_error(),
                    "libxml2's HTML serializer isn't supported; HTML documents use the HTML5 one",
                )
            })?;
        if let Some(string) = transcode(ruby, output)? {
            write_to_io(ruby, io, string)?;
        }
        Ok(io)
    }

    fn prepend_newline_p(ruby: &Ruby, rb_self: &Self) -> Result<bool, Error> {
        rb_self.with_arena(ruby, html5::prepend_newline)
    }

    /// The options are the parser options of libxml2. The HTML5 parser does
    /// not use them, but they are still converted as Nokogiri converts them.
    fn in_context(
        ruby: &Ruby,
        rb_self: &Self,
        input: Value,
        _options: i32,
    ) -> Result<Value, Error> {
        html5ever::in_context(ruby, rb_self, input)
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

    /// `Node#[]=`, native half: `nil`, and no change, unless an element.
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
    /// `Object#dup` or `#clone` has allocated the copy: `xmlDocCopyNode`. Then
    /// the object is cached and decorated. `nil` for nodes that libxml2 does
    /// not copy, such as doctypes.
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
        // `Object#dup` hands over a fresh object. Repointing a live one would
        // leave it in the cache slot of its old node.
        if rb_self.data_ptr_p() {
            return Err(Error::new(
                ruby.exception_runtime_error(),
                "node is already initialized",
            ));
        }
        let other = <&Node>::try_convert(other)?;
        let (source, id) = (other.document(ruby)?, other.id(ruby)?);
        let new_parent_doc = document::unwrap(ruby, new_parent_doc)?;
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
        Node::repoint(rb_self, new_parent_doc, copy);
        let cache = new_parent_doc.cache(ruby)?;
        cache.store(cache_index(copy), rb_self)?;
        new_parent_doc.funcall::<_, _, Value>("decorate", (rb_self,))?;
        Ok(Some(rb_self))
    }

    pub fn with_arena_mut<R: Send>(
        &self,
        ruby: &Ruby,
        f: impl FnOnce(&mut dom::Document, NodeId) -> R + Send,
    ) -> Result<R, Error> {
        let id = self.id(ruby)?;
        document::with_arena_mut(ruby, self.document(ruby)?, |arena| f(arena, id))
    }
}

/// Nokogiri's `reparent_node_with`. The core can copy a node because it is text
/// or from another document, or merge it into neighboring text. That node keeps
/// its object, which is pointed at what went into the tree. What went
/// into the tree gets an object of its own, which is decorated and returned.
/// As in Nokogiri, the original of a text node loses its cached object.
fn reparent(ruby: &Ruby, pivot: &Node, node: Value, position: Position) -> Result<Value, Error> {
    let node_class = ruby.get_inner(&NODE);
    if !node.is_kind_of(node_class) || node.is_kind_of(ruby.get_inner(&document::DOCUMENT)) {
        let message = format!("node must be a {}", node_class.inspect());
        return Err(Error::new(ruby.exception_arg_error(), message));
    }
    let reparentee = Obj::<Node>::try_convert(node)?;
    let (document, pivot_id) = (pivot.document(ruby)?, pivot.id(ruby)?);
    let (source, id) = (reparentee.document(ruby)?, reparentee.id(ruby)?);
    let namespace_inheritance = document
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
        source.cache(ruby)?.store(cache_index(id), ruby.qnil())?;
    }
    Node::repoint(reparentee, document, reparented);
    let reparented = wrap(ruby, document, reparented)?;
    reparented.funcall::<_, _, Value>("decorate!", ())?;
    Ok(reparented)
}

/// The output in the encoding that libxml2 would transcode it to, with `&#NNN;`
/// for characters that the encoding lacks, as the encoder of libxml2 writes
/// them. `None` for an encoding that Ruby does not know. libxml2 writes nothing
/// for it.
fn transcode(ruby: &Ruby, output: xml::Output) -> Result<Option<RString>, Error> {
    let string = utf8_string(ruby, output.bytes);
    let Some(label) = output.encoding else {
        return Ok(Some(string));
    };
    let Some(encoding) = std::str::from_utf8(&label)
        .ok()
        .and_then(|label| ruby.find_encindex(label).ok())
        .map(RbEncoding::from)
    else {
        return Ok(None);
    };
    if encoding.name() == ruby.utf8_encoding().name() {
        return Ok(Some(string));
    }
    let options = ruby.hash_new();
    options.aset(
        ruby.to_symbol("fallback"),
        ruby.proc_new(character_reference),
    )?;
    options.aset(ruby.to_symbol("invalid"), ruby.to_symbol("replace"))?;
    string
        .funcall("encode", (Encoding::from(encoding), KwArgs(options)))
        .map(Some)
}

fn character_reference(
    _ruby: &Ruby,
    args: &[Value],
    _block: Option<Proc>,
) -> Result<String, Error> {
    let character: u32 = args[0].funcall("ord", ())?;
    Ok(format!("&#{character};"))
}

/// `noko_io_write`: the bytes tagged with the external encoding of the IO,
/// or as binary. If the write raises a StandardError, the output ends there.
fn write_to_io(ruby: &Ruby, io: Value, string: RString) -> Result<(), Error> {
    let external_encoding: Value = if io.respond_to("external_encoding", false)? {
        io.funcall("external_encoding", ())?
    } else {
        ruby.qnil().as_value()
    };
    let encoding = if external_encoding.is_nil() {
        ruby.ascii8bit_encoding()
    } else {
        RbEncoding::try_convert(external_encoding)?
    };
    string.enc_associate(encoding)?;
    match io.funcall::<_, _, Value>("write", (string,)) {
        Err(error) if error.is_kind_of(ruby.exception_standard_error()) => Ok(()),
        written => written.map(drop),
    }
}

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

pub fn wrap_all(ruby: &Ruby, document: Obj<Node>, ids: Vec<NodeId>) -> Result<RArray, Error> {
    let nodes = ruby.ary_new_capa(ids.len());
    for id in ids {
        nodes.push(wrap(ruby, document, id)?)?;
    }
    Ok(nodes)
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

/// The one Ruby object for a node, created on first use:
/// the `noko_xml_node_wrap` of Nokogiri. The class follows the node type unless
/// `class` is given. A new object goes into the node cache of the document
/// and through `Document#decorate`. `initialize` is not called.
pub fn wrap_as(
    ruby: &Ruby,
    document: Obj<Node>,
    id: NodeId,
    class: Option<RClass>,
) -> Result<Value, Error> {
    if id == NodeId::DOCUMENT {
        return Ok(document.as_value());
    }
    let cache = document.cache(ruby)?;
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

pub fn wrap(ruby: &Ruby, document: Obj<Node>, id: NodeId) -> Result<Value, Error> {
    wrap_as(ruby, document, id, None)
}

fn cache_index(id: NodeId) -> isize {
    isize::try_from(id.index()).expect("node ids fit in an isize")
}

/// The table of Nokogiri from node type to class, for the types of an HTML
/// tree.
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

/// The document that a new node goes into. Nokogiri deprecates passing another
/// node instead. It resolves to the document of that node.
pub fn document_for_new_node(
    ruby: &Ruby,
    document: Value,
    deprecation: &str,
) -> Result<Obj<Node>, Error> {
    if document.is_kind_of(ruby.get_inner(&document::DOCUMENT)) {
        return document::unwrap(ruby, document);
    }
    warn_deprecated(deprecation)?;
    <&Node>::try_convert(document)?.document(ruby)
}

/// Shared tail of the native constructors of Nokogiri: wrap as the receiving
/// class, run `initialize` with the original arguments, then yield.
pub fn finish_new(
    ruby: &Ruby,
    document: Obj<Node>,
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
    // `Node.new` gives an Element, as the node type of libxml2 decides.
    let class = if class.equal(node_class)? {
        ruby.get_inner(&ELEMENT)
    } else {
        class
    };
    finish_new(ruby, document, id, class, args)
}

/// Names are atoms that the parser and the selector engine share, so they must
/// be UTF-8, unlike in Nokogiri (ARCHITECTURE.md, departures).
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
    class.define_private_method("in_context", method!(Node::in_context, 2))?;
    class.define_private_method("native_write_to", method!(Node::native_write_to, 4))?;
    class.define_private_method("prepend_newline?", method!(Node::prepend_newline_p, 0))?;
    class.define_private_method(
        "html_standard_serialize",
        method!(Node::html_standard_serialize, 2),
    )?;
    Ok(())
}
