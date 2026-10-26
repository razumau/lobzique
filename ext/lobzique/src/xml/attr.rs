//! `XML::Attr`: attributes are nodes, as in libxml2.

use super::node::NODE;
use crate::XML;
use magnus::{value::Lazy, Module, RClass, Ruby};

pub static ATTR: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .define_class("Attr", ruby.get_inner(&NODE))
        .unwrap()
});

pub fn init(ruby: &Ruby) {
    ruby.get_inner(&ATTR);
}
