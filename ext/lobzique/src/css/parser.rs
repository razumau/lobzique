//! `CSS::Parser#xpath_for`, which in Nokogiri parses the selector into
//! `CSS::Node`s for the visitor to walk. Here the core does both, so the
//! visitor only configures it (ARCHITECTURE.md).

use crate::CSS;
use lobzique_core::css::{self, Visitor};
use magnus::{method, prelude::*, Error, RArray, RString, Ruby, Symbol, Value};

fn xpath_for(
    ruby: &Ruby,
    _rb_self: Value,
    selector: RString,
    visitor: Value,
) -> Result<RArray, Error> {
    let rule = String::from_utf8(crate::support::string_bytes(selector))
        .map_err(|_| Error::new(ruby.exception_arg_error(), "invalid byte sequence in UTF-8"))?;
    let builtins: Symbol = visitor.funcall("builtins", ())?;
    let doctype: Symbol = visitor.funcall("doctype", ())?;
    let prefix: RString = visitor.funcall("prefix", ())?;
    let namespaces: Value = visitor.funcall("namespaces", ())?;
    let visitor = Visitor {
        // OPTIMAL asks Nokogiri.uses_libxml?, and lobzique has the builtins
        builtins: builtins.name()? != "never",
        html5: doctype.name()? == "html5",
        default_namespace: !namespaces.is_nil()
            && namespaces.funcall::<_, _, bool>("key?", ("xmlns",))?,
        prefix: prefix.to_string()?,
    };
    let paths = css::xpath_for(&rule, &visitor).map_err(|error| {
        match ruby.get_inner(&CSS).const_get("SyntaxError") {
            Ok(class) => Error::new(class, error.0),
            Err(error) => error,
        }
    })?;
    Ok(ruby.ary_from_iter(paths.iter().map(|path| ruby.str_new(path))))
}

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let class = ruby
        .get_inner(&CSS)
        .define_class("Parser", ruby.class_object())?;
    class.define_method("xpath_for", method!(xpath_for, 2))?;
    Ok(())
}
