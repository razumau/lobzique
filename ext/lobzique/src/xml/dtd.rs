//! `XML::DTD`: a document's doctype.

use super::node::NODE;
use crate::XML;
use magnus::{value::Lazy, Module, RClass, Ruby};

pub static DTD: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .define_class("DTD", ruby.get_inner(&NODE))
        .unwrap()
});

pub fn init(ruby: &Ruby) {
    ruby.get_inner(&DTD);
}
