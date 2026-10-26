//! `XML::Comment`.

use super::document;
use super::node::{finish_new, NODE};
use super::CHARACTER_DATA;
use crate::support::{c_string, check_string};
use crate::XML;
use magnus::{method, prelude::*, value::Lazy, Error, RClass, Ruby, Value};

pub static COMMENT: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .define_class("Comment", ruby.get_inner(&CHARACTER_DATA))
        .unwrap()
});

/// `Comment.new(document_or_node, content, *rest)`.
fn new(ruby: &Ruby, class: RClass, args: &[Value]) -> Result<Value, Error> {
    ruby.check_arity(args.len(), 2..)?;
    check_string(ruby, args[1])?;
    if !args[0].is_kind_of(ruby.get_inner(&NODE)) {
        return Err(Error::new(
            ruby.exception_arg_error(),
            "first argument must be a XML::Document or XML::Node",
        ));
    }
    let document: Value = args[0].funcall("document", ())?;
    let content = c_string(args[1])?;
    let id = document::with_arena_mut(ruby, document, |arena| arena.create_comment(content))?;
    finish_new(ruby, document, id, class, args)
}

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let class = ruby.get_inner(&COMMENT);
    class.define_singleton_method("new", method!(new, -1))?;
    Ok(())
}
