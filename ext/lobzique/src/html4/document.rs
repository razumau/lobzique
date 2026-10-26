//! `HTML4::Document`.

use crate::support::{call_initialize, optional_arg};
use crate::xml::document::{wrap_document, DOCUMENT};
use crate::HTML4;
use lobzique_core::dom;
use magnus::{method, prelude::*, value::Lazy, Error, RClass, RString, Ruby, Value};

pub static HTML4_DOCUMENT: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&HTML4)
        .define_class("Document", ruby.get_inner(&DOCUMENT))
        .unwrap()
});

/// `HTML4::Document.new(uri = nil, external_id = nil, *rest)`, which starts
/// with libxml2's default HTML 4.0 doctype; `HTML5::Document.new` inherits it.
fn new(ruby: &Ruby, class: RClass, args: &[Value]) -> Result<Value, Error> {
    let truthy_string = |index| -> Result<Option<String>, Error> {
        let value = optional_arg(ruby, args, index);
        if value.to_bool() {
            Ok(Some(RString::try_convert(value)?.to_string()?))
        } else {
            Ok(None)
        }
    };
    let arena = dom::Document::new_html4(truthy_string(0)?, truthy_string(1)?);
    let document = wrap_document(ruby, class, arena)?;
    call_initialize(document, args)?;
    Ok(document)
}

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let class = ruby.get_inner(&HTML4_DOCUMENT);
    class.define_singleton_method("new", method!(new, -1))?;
    Ok(())
}
