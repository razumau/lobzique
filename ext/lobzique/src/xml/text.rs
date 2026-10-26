//! `XML::Text`.

use super::node::{document_for_new_node, finish_new, NODE};
use super::{document, CHARACTER_DATA};
use crate::support::{c_string, check_string, obj_class};
use crate::XML;
use magnus::{method, prelude::*, value::Lazy, Error, RClass, Ruby, Value};

pub static TEXT: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .define_class("Text", ruby.get_inner(&CHARACTER_DATA))
        .unwrap()
});

/// `Text.new(string, document, *rest)`.
fn new(ruby: &Ruby, class: RClass, args: &[Value]) -> Result<Value, Error> {
    ruby.check_arity(args.len(), 2..)?;
    check_string(ruby, args[0])?;
    let document = expect_document_or_node(ruby, args[1], "second")?;
    let document = document_for_new_node(
        ruby,
        document,
        "Passing a Node as the second parameter to Text.new is deprecated. Please pass a Document instead.",
    )?;
    let content = c_string(args[0])?;
    let id = document::with_arena_mut(ruby, document, |arena| arena.create_text(content))?;
    finish_new(ruby, document, id, class, args)
}

pub fn expect_document_or_node(ruby: &Ruby, value: Value, position: &str) -> Result<Value, Error> {
    if value.is_kind_of(ruby.get_inner(&NODE)) {
        return Ok(value);
    }
    let message = format!(
        "expected {position} parameter to be a {}, received {}",
        ruby.get_inner(&document::DOCUMENT).inspect(),
        obj_class(value).inspect()
    );
    Err(Error::new(ruby.exception_type_error(), message))
}

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let class = ruby.get_inner(&TEXT);
    class.define_singleton_method("new", method!(new, -1))?;
    Ok(())
}
