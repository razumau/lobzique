//! `XML::SyntaxError` and its subclasses, made as Nokogiri's
//! `noko_xml_syntax_error__wrap` makes them from libxml2's `xmlError`.

use super::node::utf8_string;
use magnus::{prelude::*, Error, Exception, RClass, RObject, Ruby, Value};

/// The parts of libxml2's `xmlError` that an error here has.
pub struct XmlError<'a> {
    pub domain: i32,
    pub code: i32,
    pub message: &'a str,
    pub level: i32,
    pub file: Value,
    pub line: u64,
    pub str1: Option<&'a [u8]>,
}

/// An instance of `class`, with every instance variable Nokogiri sets.
pub fn wrap(ruby: &Ruby, class: RClass, error: &XmlError) -> Result<Exception, Error> {
    let exception: Value = class.new_instance((utf8_string(ruby, error.message),))?;
    let object = RObject::from_value(exception).expect("an exception is an object");
    object.ivar_set("@domain", error.domain)?;
    object.ivar_set("@code", error.code)?;
    object.ivar_set("@level", error.level)?;
    object.ivar_set("@file", error.file)?;
    object.ivar_set("@line", error.line)?;
    object.ivar_set("@str1", error.str1.map(|str1| utf8_string(ruby, str1)))?;
    object.ivar_set("@str2", ruby.qnil())?;
    object.ivar_set("@str3", ruby.qnil())?;
    object.ivar_set("@int1", 0)?;
    object.ivar_set("@column", 0)?;
    Ok(Exception::from_value(exception).expect("an instance of an exception class"))
}
