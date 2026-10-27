pub mod attr;
pub mod cdata;
pub mod comment;
pub mod document;
pub mod document_fragment;
pub mod dtd;
pub mod namespace;
pub mod node;
pub mod text;

use crate::XML;
use magnus::{value::Lazy, Error, Module, RClass, Ruby};
use node::NODE;

// Nokogiri defines these two in nokogiri.c, with no native methods
pub static ELEMENT: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .define_class("Element", ruby.get_inner(&NODE))
        .unwrap()
});
pub static CHARACTER_DATA: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .define_class("CharacterData", ruby.get_inner(&NODE))
        .unwrap()
});

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    namespace::init(ruby)?;
    node::init(ruby)?;
    attr::init(ruby);
    dtd::init(ruby)?;
    ruby.get_inner(&ELEMENT);
    ruby.get_inner(&CHARACTER_DATA);
    comment::init(ruby)?;
    text::init(ruby)?;
    cdata::init(ruby)?;
    document_fragment::init(ruby)?;
    document::init(ruby)?;
    Ok(())
}
