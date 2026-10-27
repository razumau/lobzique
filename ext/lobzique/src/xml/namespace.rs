//! `XML::Namespace`: a node's namespace, as libxml2 records it.

use super::node::utf8_string;
use crate::XML;
use magnus::{
    method, prelude::*, value::Lazy, DataTypeFunctions, Error, RClass, RString, Ruby, TypedData,
    Value,
};

pub static NAMESPACE: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .define_class("Namespace", ruby.class_object())
        .unwrap()
});

/// A copy of the prefix and URI. libxml2 shares one `xmlNs` among the nodes
/// that use it, and Nokogiri one object; here each call makes a new one
/// (ARCHITECTURE.md, departures).
#[derive(TypedData)]
#[magnus(class = "Lobzique::XML::Namespace", free_immediately)]
pub struct Namespace {
    prefix: Option<String>,
    href: String,
}

impl DataTypeFunctions for Namespace {}

impl Namespace {
    fn prefix(ruby: &Ruby, rb_self: &Self) -> Option<RString> {
        rb_self
            .prefix
            .as_ref()
            .map(|prefix| utf8_string(ruby, prefix))
    }

    fn href(ruby: &Ruby, rb_self: &Self) -> RString {
        utf8_string(ruby, &rb_self.href)
    }
}

/// `noko_xml_namespace_wrap` for a namespace in `document`.
pub fn wrap(
    ruby: &Ruby,
    document: Value,
    prefix: Option<String>,
    href: String,
) -> Result<Value, Error> {
    let namespace = ruby.obj_wrap_as(Namespace { prefix, href }, ruby.get_inner(&NAMESPACE));
    namespace.ivar_set("@document", document)?;
    Ok(namespace.as_value())
}

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let class = ruby.get_inner(&NAMESPACE);
    class.undef_default_alloc_func();
    class.define_method("prefix", method!(Namespace::prefix, 0))?;
    class.define_method("href", method!(Namespace::href, 0))?;
    Ok(())
}
