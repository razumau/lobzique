//! `XML::CDATA`.

use super::document;
use super::node::{document_for_new_node, finish_new};
use super::text::{expect_document_or_node, expect_string, TEXT};
use crate::XML;
use magnus::{method, prelude::*, value::Lazy, Error, RClass, Ruby, Value};

pub static CDATA: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .define_class("CDATA", ruby.get_inner(&TEXT))
        .unwrap()
});

/// `CDATA.new(document, content, *rest)`.
fn new(ruby: &Ruby, class: RClass, args: &[Value]) -> Result<Value, Error> {
    ruby.check_arity(args.len(), 2..)?;
    let content = expect_string(ruby, args[1])?;
    let document = expect_document_or_node(ruby, args[0], "first")?;
    let document = document_for_new_node(
        ruby,
        document,
        "Passing a Node as the first parameter to CDATA.new is deprecated. Please pass a Document instead.",
    )?;
    let id = document::with_arena_mut(ruby, document, |arena| arena.create_cdata(content))?;
    finish_new(ruby, document, id, class, args)
}

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let class = ruby.get_inner(&CDATA);
    class.define_singleton_method("new", method!(new, -1))?;
    Ok(())
}
