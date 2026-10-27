//! `XML::Attr`: attributes are nodes, as in libxml2.

use super::node::{Node, NODE};
use crate::support::c_string;
use crate::XML;
use magnus::{method, prelude::*, value::Lazy, Error, Module, RClass, Ruby, Value};

pub static ATTR: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .define_class("Attr", ruby.get_inner(&NODE))
        .unwrap()
});

/// `Attr#value=`; `nil` empties the value.
fn set_value(ruby: &Ruby, rb_self: &Node, content: Value) -> Result<Value, Error> {
    let value = if content.is_nil() {
        Vec::new()
    } else {
        c_string(content)?
    };
    rb_self.with_arena_mut(ruby, move |arena, id| arena.set_attribute_value(id, value))?;
    Ok(content)
}

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let class = ruby.get_inner(&ATTR);
    class.define_method("value=", method!(set_value, 1))?;
    Ok(())
}
