//! `XML::DTD`: a document's doctype.

use super::node::{utf8_string, Node, NODE};
use crate::XML;
use lobzique_core::dom::{Doctype, NodeKind};
use magnus::{method, value::Lazy, Error, Module, RClass, RString, Ruby};

pub static DTD: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .define_class("DTD", ruby.get_inner(&NODE))
        .unwrap()
});

fn doctype_field(
    ruby: &Ruby,
    rb_self: &Node,
    field: fn(&Doctype) -> &Option<Vec<u8>>,
) -> Result<Option<RString>, Error> {
    let value = rb_self.with_arena(ruby, move |arena, id| match arena.node_kind(id) {
        NodeKind::Doctype(doctype) => field(doctype).clone(),
        _ => None,
    })?;
    Ok(value.map(|value| utf8_string(ruby, value)))
}

fn system_id(ruby: &Ruby, rb_self: &Node) -> Result<Option<RString>, Error> {
    doctype_field(ruby, rb_self, |doctype| &doctype.system_id)
}

fn external_id(ruby: &Ruby, rb_self: &Node) -> Result<Option<RString>, Error> {
    doctype_field(ruby, rb_self, |doctype| &doctype.public_id)
}

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let class = ruby.get_inner(&DTD);
    class.define_method("system_id", method!(system_id, 0))?;
    class.define_method("external_id", method!(external_id, 0))?;
    Ok(())
}
