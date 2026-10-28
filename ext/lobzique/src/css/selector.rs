//! `CSS::Selector`: a selector list compiled into the query Nokogiri's
//! `XPathVisitor` would write as an XPath string. Nokogiri has no such
//! class; `Searchable` makes one where it would make that string, and
//! `XPathContext#evaluate` runs it (ARCHITECTURE.md).

use crate::CSS;
use lobzique_core::css::{self, Options, Prefix};
use magnus::{
    function, prelude::*, scan_args::get_kwargs, scan_args::scan_args, value::Lazy, Error, RArray,
    RClass, RHash, RString, Ruby, Symbol, TypedData, Value,
};

pub static SELECTOR: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&CSS)
        .define_class("Selector", ruby.class_object())
        .unwrap()
});

#[derive(TypedData)]
#[magnus(class = "Lobzique::CSS::Selector", free_immediately)]
pub struct Selector {
    pub compiled: css::Selector,
    /// The selector as written, which XPath errors quote.
    pub rule: String,
}

impl magnus::DataTypeFunctions for Selector {}

/// `CSS::Selector.new(rule, prefixes:, doctype:, namespaces: nil,
/// builtins: true, first: false)`, the arguments of Nokogiri's
/// `XPathVisitor.new` and `CSS.xpath_for`. `doctype:` is what
/// `Document#xpath_doctype` returns.
fn new(ruby: &Ruby, args: &[Value]) -> Result<Selector, Error> {
    let args = scan_args::<(RString,), (), (), (), RHash, ()>(args)?;
    let keywords =
        get_kwargs::<_, (RArray, Symbol), (Option<Value>, Option<bool>, Option<bool>), ()>(
            args.keywords,
            &["prefixes", "doctype"],
            &["namespaces", "builtins", "first"],
        )?;
    let (prefixes, doctype) = keywords.required;
    let (namespaces, builtins, first) = keywords.optional;
    let rule = String::from_utf8(crate::support::string_bytes(args.required.0))
        .map_err(|_| Error::new(ruby.exception_arg_error(), "invalid byte sequence in UTF-8"))?;
    let prefixes = prefixes
        .into_iter()
        .map(|prefix| {
            let prefix = RString::try_convert(prefix)?.to_string()?;
            Prefix::parse(&prefix).ok_or_else(|| {
                Error::new(
                    ruby.exception_arg_error(),
                    format!("unknown XPath prefix {prefix:?}"),
                )
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    // HTML4 documents parse as HTML5 (plan.md decision 3), so their names
    // have HTML5's namespaces to ignore
    let html = match doctype.name()?.as_ref() {
        "xml" => false,
        "html4" | "html5" => true,
        _ => {
            let message = format!(
                "Invalid values {} for doctype: keyword parameter",
                doctype.inspect()
            );
            return Err(Error::new(ruby.exception_arg_error(), message));
        }
    };
    let default_namespace = match namespaces.filter(|namespaces| !namespaces.is_nil()) {
        Some(namespaces) => namespaces.funcall::<_, _, bool>("key?", ("xmlns",))?,
        None => false,
    };
    let options = Options {
        html,
        builtins: builtins.unwrap_or(true),
        default_namespace,
        prefixes,
        first_only: first.unwrap_or(false),
    };
    let compiled = css::compile(&rule, &options).map_err(|error| match syntax_error(ruby) {
        Ok(class) => Error::new(class, error.0),
        Err(error) => error,
    })?;
    Ok(Selector { compiled, rule })
}

fn syntax_error(ruby: &Ruby) -> Result<magnus::ExceptionClass, Error> {
    ruby.get_inner(&CSS).const_get("SyntaxError")
}

fn rule(ruby: &Ruby, rb_self: &Selector) -> RString {
    ruby.str_new(&rb_self.rule)
}

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let class = ruby.get_inner(&SELECTOR);
    class.define_singleton_method("new", function!(new, -1))?;
    class.define_method("to_s", magnus::method!(rule, 0))?;
    Ok(())
}
