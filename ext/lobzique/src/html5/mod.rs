use crate::html4::document::HTML4_DOCUMENT;
use crate::HTML5;
use magnus::{value::Lazy, Error, Module, RClass, Ruby};

// Native so that its superclass is fixed before the Ruby files load, as in
// Nokogiri's gumbo.c; all of its methods are Ruby.
pub static HTML5_DOCUMENT: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&HTML5)
        .define_class("Document", ruby.get_inner(&HTML4_DOCUMENT))
        .unwrap()
});

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    ruby.get_inner(&HTML5_DOCUMENT);
    Ok(())
}
