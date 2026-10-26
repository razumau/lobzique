//! `XML::DocumentFragment`. Ruby's `DocumentFragment.new` calls
//! `native_new` and then `initialize`, so subclasses only override the latter.

use super::document;
use super::node::{wrap_as, NODE};
use crate::XML;
use magnus::{method, prelude::*, value::Lazy, Error, RClass, Ruby, Value};

pub static DOCUMENT_FRAGMENT: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .define_class("DocumentFragment", ruby.get_inner(&NODE))
        .unwrap()
});

/// `DocumentFragment.native_new(document)`: a fragment of the receiving
/// class, wrapped and decorated, before `initialize`.
fn native_new(ruby: &Ruby, class: RClass, document: Value) -> Result<Value, Error> {
    let id = document::with_arena_mut(ruby, document, |arena| arena.create_fragment())?;
    wrap_as(ruby, document, id, Some(class))
}

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let class = ruby.get_inner(&DOCUMENT_FRAGMENT);
    class.define_singleton_method("native_new", method!(native_new, 1))?;
    Ok(())
}
