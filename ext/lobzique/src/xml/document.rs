//! `XML::Document`: the node object that owns its document's arena.

use super::node::{utf8_string, wrap, Node, NODE};
use super::ELEMENT;
use crate::support::{c_string, call_initialize, displayed_class, obj_class, optional_arg};
use crate::XML;
use lobzique_core::dom::{self, CopyDepth, DocumentKind, NodeId, NodeType};
use magnus::{
    method, prelude::*, rb_sys::AsRawValue, typed_data::Obj, value::Lazy, Error, RClass, RString,
    Ruby, Value,
};

pub static DOCUMENT: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .define_class("Document", ruby.get_inner(&NODE))
        .unwrap()
});

/// A document object of `class` that owns `arena`, before `initialize` has
/// run: Nokogiri's `noko_xml_document_wrap`.
pub fn wrap_document(ruby: &Ruby, class: RClass, arena: dom::Document) -> Result<Value, Error> {
    let document = ruby.obj_wrap_as(Node::new_document(arena), class);
    document.set_document(document.as_value());
    document.ivar_set("@node_cache", ruby.ary_new())?;
    document.ivar_set("@decorators", ruby.qnil())?;
    document.ivar_set("@errors", ruby.qnil())?;
    Ok(document.as_value())
}

/// `Document.new(version = "1.0", *rest)`.
fn new(ruby: &Ruby, class: RClass, args: &[Value]) -> Result<Value, Error> {
    let version = optional_arg(ruby, args, 0);
    let version = if version.is_nil() {
        b"1.0".to_vec()
    } else {
        c_string(version)?
    };
    let mut arena = dom::Document::new(DocumentKind::Xml);
    arena.set_version(Some(version));
    let document = wrap_document(ruby, class, arena)?;
    call_initialize(document, args)?;
    Ok(document)
}

/// The data of a document object; raises the way `noko_xml_document_unwrap` does.
pub fn unwrap(ruby: &Ruby, document: Value) -> Result<&Node, Error> {
    let class = ruby.get_inner(&DOCUMENT);
    if !document.is_kind_of(class) {
        let message = format!(
            "wrong argument type {} (expected {})",
            displayed_class(document),
            class.inspect()
        );
        return Err(Error::new(ruby.exception_type_error(), message));
    }
    <&Node>::try_convert(document)
}

fn arena<'a>(
    ruby: &Ruby,
    document: &'a Node,
) -> Result<&'a std::cell::RefCell<dom::Document>, Error> {
    document.arena().ok_or_else(|| {
        Error::new(
            ruby.exception_runtime_error(),
            "Uninitialized document struct (null data pointer)",
        )
    })
}

/// Runs `f` on the document's arena; the borrow ends when `f` returns.
///
/// `f` can't reach Ruby while the arena is borrowed: Ruby handles and
/// values aren't `Send`, so `f` can neither capture nor return one, and
/// the lints in `lib.rs` and `clippy.toml` close the ways to get a handle
/// inside it. A Ruby exception can therefore never unwind past the borrow
/// and leave the document locked.
pub fn with_arena<R: Send>(
    ruby: &Ruby,
    document: Value,
    f: impl FnOnce(&dom::Document) -> R + Send,
) -> Result<R, Error> {
    let arena = arena(ruby, unwrap(ruby, document)?)?;
    let arena = arena.try_borrow().map_err(|_| busy(ruby))?;
    Ok(f(&arena))
}

/// [`with_arena`] with a mutable borrow.
pub fn with_arena_mut<R: Send>(
    ruby: &Ruby,
    document: Value,
    f: impl FnOnce(&mut dom::Document) -> R + Send,
) -> Result<R, Error> {
    let arena = arena(ruby, unwrap(ruby, document)?)?;
    let mut arena = arena.try_borrow_mut().map_err(|_| busy(ruby))?;
    Ok(f(&mut arena))
}

/// [`with_arena_mut`] on two different documents at once.
pub fn with_arenas_mut<R: Send>(
    ruby: &Ruby,
    first: Value,
    second: Value,
    f: impl FnOnce(&mut dom::Document, &mut dom::Document) -> R + Send,
) -> Result<R, Error> {
    let first = arena(ruby, unwrap(ruby, first)?)?;
    let second = arena(ruby, unwrap(ruby, second)?)?;
    let mut first = first.try_borrow_mut().map_err(|_| busy(ruby))?;
    let mut second = second.try_borrow_mut().map_err(|_| busy(ruby))?;
    Ok(f(&mut first, &mut second))
}

// Unreachable while no borrow outlives a call into Ruby; a guard, not a feature
fn busy(ruby: &Ruby) -> Error {
    Error::new(
        ruby.exception_runtime_error(),
        "document is in use by a native call",
    )
}

/// libxml2's `xmlDocGetRootElement`: the first element child.
fn root(ruby: &Ruby, rb_self: Value) -> Result<Option<Value>, Error> {
    let root = with_arena(ruby, rb_self, |arena| {
        arena.element_children(NodeId::DOCUMENT).next()
    })?;
    root.map(|id| wrap(ruby, rb_self, id)).transpose()
}

/// `Document#root=`. A root from another document is copied, and the
/// argument returned as given, still in its own document.
fn set_root(ruby: &Ruby, rb_self: Value, new_root: Value) -> Result<Value, Error> {
    unwrap(ruby, rb_self)?;
    if new_root.is_nil() {
        with_arena_mut(ruby, rb_self, |arena| arena.set_root(None))?
            .expect("no root is always allowed");
        return Ok(new_root);
    }
    let node_class = ruby.get_inner(&NODE);
    if !new_root.is_kind_of(node_class) {
        let message = format!(
            "expected {} but received {}",
            node_class.inspect(),
            obj_class(new_root).inspect()
        );
        return Err(Error::new(ruby.exception_arg_error(), message));
    }
    let root = <&Node>::try_convert(new_root)?;
    let (root_document, id) = (root.document(ruby)?, root.id(ruby)?);
    if root.with_arena(ruby, |arena, id| arena.node_type(id))? != NodeType::Element {
        let message = format!("root must be a {}", ruby.get_inner(&ELEMENT).inspect());
        return Err(Error::new(ruby.exception_type_error(), message));
    }
    let set = if root_document.as_raw() == rb_self.as_raw() {
        with_arena_mut(ruby, rb_self, |arena| arena.set_root(Some(id)))?
    } else {
        with_arenas_mut(ruby, rb_self, root_document, |arena, source| {
            let copy = arena.import(source, id, CopyDepth::Deep);
            arena.set_root(copy)
        })?
    };
    set.expect("checked to be an element");
    Ok(new_root)
}

/// `Document#initialize_copy_with_args(other, level)`, after `Object#dup`
/// has allocated the copy and copied the instance variables: the arena is
/// copied, as `xmlCopyDoc` copies the document, with a new node cache.
fn initialize_copy_with_args(
    ruby: &Ruby,
    rb_self: Obj<Node>,
    other: Value,
    level: i32,
) -> Result<Value, Error> {
    let copy = with_arena(ruby, other, move |arena| arena.copy_document(level != 0))?;
    if !rb_self.adopt_arena(copy) {
        return Err(Error::new(
            ruby.exception_runtime_error(),
            "document is already initialized",
        ));
    }
    rb_self.set_document(rb_self.as_value());
    rb_self.ivar_set("@node_cache", ruby.ary_new())?;
    Ok(rb_self.as_value())
}

fn encoding(ruby: &Ruby, rb_self: Value) -> Result<Option<RString>, Error> {
    let encoding = with_arena(ruby, rb_self, |arena| arena.encoding().map(<[u8]>::to_vec))?;
    Ok(encoding.map(|encoding| utf8_string(ruby, encoding)))
}

fn set_encoding(ruby: &Ruby, rb_self: Value, encoding: Value) -> Result<Value, Error> {
    let label = c_string(encoding)?;
    with_arena_mut(ruby, rb_self, |arena| arena.set_encoding(Some(label)))?;
    Ok(encoding)
}

fn version(ruby: &Ruby, rb_self: Value) -> Result<Option<RString>, Error> {
    let version = with_arena(ruby, rb_self, |arena| arena.version().map(<[u8]>::to_vec))?;
    Ok(version.map(|version| utf8_string(ruby, version)))
}

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let class = ruby.get_inner(&DOCUMENT);
    class.define_singleton_method("new", method!(new, -1))?;
    class.define_method("root", method!(root, 0))?;
    class.define_method("root=", method!(set_root, 1))?;
    class.define_method("encoding", method!(encoding, 0))?;
    class.define_method("encoding=", method!(set_encoding, 1))?;
    class.define_method("version", method!(version, 0))?;
    class.define_protected_method(
        "initialize_copy_with_args",
        method!(initialize_copy_with_args, 2),
    )?;
    Ok(())
}
