//! `XML::Document`: the node object that owns its document's arena.

use super::node::{Node, NODE};
use crate::support::{call_initialize, displayed_class};
use crate::XML;
use lobzique_core::dom::{self, DocumentKind};
use magnus::{method, prelude::*, value::Lazy, Error, RClass, Ruby, Value};

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

/// `Document.new(*args)`. Nokogiri's version argument is accepted and
/// ignored; only XML documents have one.
fn new(ruby: &Ruby, class: RClass, args: &[Value]) -> Result<Value, Error> {
    let document = wrap_document(ruby, class, dom::Document::new(DocumentKind::Xml))?;
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
/// values aren't `Send`, so `f` can neither capture nor return one. A Ruby
/// exception can therefore never unwind past the borrow and leave the
/// document locked.
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

// Unreachable while no borrow outlives a call into Ruby; a guard, not a feature
fn busy(ruby: &Ruby) -> Error {
    Error::new(
        ruby.exception_runtime_error(),
        "document is in use by a native call",
    )
}

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let class = ruby.get_inner(&DOCUMENT);
    class.define_singleton_method("new", method!(new, -1))?;
    Ok(())
}
