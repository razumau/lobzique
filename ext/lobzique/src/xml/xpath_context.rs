//! `XML::XPathContext`: a context node, namespace and variable bindings,
//! and `evaluate`. It evaluates the selectors `Searchable` compiles CSS
//! into, and XPath strings in the subset of plan.md decision 2.

use super::document;
use super::node::{utf8_string, Node};
use super::node_set::{self, NODE_SET};
use super::syntax_error::{self, XmlError};
use crate::css::selector::Selector;
use crate::support::{apply, c_string, optional_arg};
use crate::XML;
use lobzique_core::dom::NodeId;
use lobzique_core::xpath::{self, Call, Outcome, ParseError, Query};
use magnus::{
    function, gc, method, prelude::*, rb_sys::AsRawValue, typed_data::Obj, value::Lazy,
    value::Opaque, DataTypeFunctions, Error, Exception, Float, Integer, IntoValue, RArray, RClass,
    RString, Ruby, TypedData, Value,
};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

pub static XPATH_CONTEXT: Lazy<RClass> = Lazy::new(|ruby| {
    ruby.get_inner(&XML)
        .define_class("XPathContext", ruby.class_object())
        .unwrap()
});

#[derive(TypedData)]
#[magnus(class = "Lobzique::XML::XPathContext", mark, compact, free_immediately)]
pub struct XPathContext {
    /// The context node's object, which follows the node across documents.
    node: Cell<Opaque<Value>>,
    namespaces: RefCell<HashMap<String, String>>,
    variables: RefCell<HashMap<Vec<u8>, Vec<u8>>>,
}

impl DataTypeFunctions for XPathContext {
    fn mark(&self, marker: &gc::Marker) {
        marker.mark_movable(self.node.get());
    }

    fn compact(&self, compactor: &gc::Compactor) {
        self.node.set(compactor.location(self.node.get()));
    }
}

/// `XPathContext.new(node)`
fn new(node: Value) -> Result<XPathContext, Error> {
    <&Node>::try_convert(node)?;
    Ok(XPathContext {
        node: Cell::new(node.into()),
        namespaces: RefCell::new(HashMap::new()),
        variables: RefCell::new(HashMap::new()),
    })
}

impl XPathContext {
    fn set_node(rb_self: &Self, node: Value) -> Result<Value, Error> {
        <&Node>::try_convert(node)?;
        rb_self.node.set(node.into());
        Ok(node)
    }

    /// `register_ns(prefix, uri)`; a `nil` URI unbinds the prefix.
    fn register_ns(rb_self: Obj<Self>, prefix: Value, uri: Value) -> Result<Obj<Self>, Error> {
        let prefix = String::from_utf8_lossy(&c_string(prefix)?).into_owned();
        let mut namespaces = rb_self.namespaces.borrow_mut();
        if uri.is_nil() {
            namespaces.remove(&prefix);
        } else {
            let uri = String::from_utf8_lossy(&c_string(uri)?).into_owned();
            namespaces.insert(prefix, uri);
        }
        drop(namespaces);
        Ok(rb_self)
    }

    /// `register_variable(name, value)`: a string variable, or none for
    /// `nil`.
    fn register_variable(
        rb_self: Obj<Self>,
        name: Value,
        value: Value,
    ) -> Result<Obj<Self>, Error> {
        let name = c_string(name)?;
        let mut variables = rb_self.variables.borrow_mut();
        if value.is_nil() {
            variables.remove(&name);
        } else {
            variables.insert(name, c_string(value)?);
        }
        drop(variables);
        Ok(rb_self)
    }

    /// `evaluate(expression, handler = nil)`. Custom functions are the
    /// handler's methods, called with the arguments libxml2 would pass, but
    /// between evaluations rather than during one: each evaluation notes
    /// the calls it has no answer for, and runs again once they are made
    /// (ARCHITECTURE.md, departures).
    fn evaluate(ruby: &Ruby, rb_self: &Self, args: &[Value]) -> Result<Value, Error> {
        ruby.check_arity(args.len(), 1..=2)?;
        let handler = optional_arg(ruby, args, 1);
        let (selector, expression, parsed);
        let (query, rule): (&Query, &[u8]) = match Obj::<Selector>::try_convert(args[0]) {
            Ok(compiled) => {
                selector = compiled;
                let rule = selector.rule.as_bytes();
                match &selector.compiled.query {
                    Ok(query) => (query, rule),
                    Err(error) => return Err(xpath_error(ruby, *error, rule)),
                }
            }
            Err(_) => {
                expression = c_string(args[0])?;
                parsed = xpath::parse(&expression).map_err(|error| match error {
                    ParseError::Syntax(error) => xpath_error(ruby, error, &expression),
                    ParseError::Unsupported(construct) => {
                        unsupported_error(ruby, &construct, &expression)
                    }
                })?;
                (&parsed, &expression)
            }
        };
        let node = <&Node>::try_convert(ruby.get_inner(rb_self.node.get()))?;
        let (document, id) = (node.document(ruby)?, node.id(ruby)?);
        let namespaces = rb_self.namespaces.borrow().clone();
        let mut answers = HashMap::new();
        loop {
            let outcome = document::with_arena(ruby, document, |arena| {
                xpath::evaluate(arena, query, id, &namespaces, &answers)
            })?
            .map_err(|error| xpath_error(ruby, error, rule))?;
            match outcome {
                Outcome::Nodes(ids) => return node_set::wrap(ruby, document, ids),
                Outcome::Calls(calls) => {
                    for call in calls {
                        let key = (call.site, call.node);
                        let answer = call_handler(ruby, document, handler, query, call, rule)?;
                        answers.insert(key, answer);
                    }
                }
            }
        }
    }
}

/// Nokogiri's `Nokogiri_marshal_xpath_funcall_and_return_values`.
fn call_handler(
    ruby: &Ruby,
    document: Value,
    handler: Value,
    query: &Query,
    call: Call,
    rule: &[u8],
) -> Result<xpath::Value<'static>, Error> {
    let name = query.function_name(call.site);
    if handler.is_nil() || !handler.respond_to(name, false)? {
        return Err(xpath_error(ruby, xpath::Error::UnregisteredFunction, rule));
    }
    let arguments = ruby.ary_new_capa(call.arguments.len());
    for argument in call.arguments {
        arguments.push(to_ruby(ruby, document, argument)?)?;
    }
    let result = apply(ruby, handler, name, arguments)?;
    if result.is_nil() {
        return Err(xpath_error(ruby, xpath::Error::StackUsage, rule));
    }
    if Float::from_value(result).is_some() || Integer::from_value(result).is_some() {
        return Ok(xpath::Value::Number(f64::try_convert(result)?));
    }
    if RString::from_value(result).is_some() {
        return Ok(xpath::Value::String(Cow::Owned(c_string(result)?)));
    }
    if result.is_kind_of(ruby.class_true_class()) || result.is_kind_of(ruby.class_false_class()) {
        return Ok(xpath::Value::Boolean(result.to_bool()));
    }
    let node_set_class = ruby.get_inner(&NODE_SET);
    let set = if RArray::from_value(result).is_some() {
        node_set_class.new_instance((document, result))?
    } else if result.is_kind_of(node_set_class) {
        result
    } else {
        return Err(Error::new(
            ruby.exception_runtime_error(),
            "Invalid return type",
        ));
    };
    let members: RArray = set.funcall("to_a", ())?;
    let mut ids = Vec::with_capacity(members.len());
    for member in members {
        let Ok(node) = <&Node>::try_convert(member) else {
            continue; // a Namespace
        };
        if node.document(ruby)?.as_raw() != document.as_raw() {
            return Err(Error::new(
                ruby.exception_runtime_error(),
                "Invalid return type: node from another document",
            ));
        }
        ids.push(node.id(ruby)?);
    }
    let ids = document::with_arena(ruby, document, move |arena| {
        let mut ids: Vec<NodeId> = ids;
        arena.sort_in_document_order(&mut ids);
        ids
    })?;
    Ok(xpath::Value::NodeSet(ids))
}

/// Nokogiri's `_noko_xml_xpath_context__xpath2ruby`.
fn to_ruby(ruby: &Ruby, document: Value, value: xpath::Value<'static>) -> Result<Value, Error> {
    Ok(match value {
        xpath::Value::NodeSet(ids) => node_set::wrap(ruby, document, ids)?,
        xpath::Value::Boolean(b) => b.into_value_with(ruby),
        xpath::Value::Number(n) => ruby.float_from_f64(n).as_value(),
        xpath::Value::String(s) => utf8_string(ruby, s).as_value(),
    })
}

/// An `XPath::SyntaxError` as Nokogiri makes one from libxml2's error:
/// level ERROR, domain XPath, the expression in `str1`. For CSS, lobzique
/// quotes the rule, where Nokogiri quotes the XPath it became.
fn xpath_error(ruby: &Ruby, error: xpath::Error, rule: &[u8]) -> Error {
    syntax_error(ruby, "SyntaxError", error.message(), error.code(), rule)
}

/// `XPath::UnsupportedError`: valid XPath outside the subset, raised as
/// libxml2 would raise an invalid expression, naming the construct.
fn unsupported_error(ruby: &Ruby, construct: &str, expression: &[u8]) -> Error {
    let message = format!("Unsupported XPath: {construct}");
    let code = xpath::Error::InvalidExpression.code();
    syntax_error(ruby, "UnsupportedError", &message, code, expression)
}

fn syntax_error(ruby: &Ruby, class: &str, message: &str, code: i32, rule: &[u8]) -> Error {
    let build = || -> Result<Exception, Error> {
        let class: RClass = ruby
            .get_inner(&XML)
            .const_get::<_, magnus::RModule>("XPath")?
            .const_get(class)?;
        let error = XmlError {
            domain: 12, // XML_FROM_XPATH
            code,
            message,
            level: 2, // XML_ERR_ERROR
            file: ruby.qnil().as_value(),
            line: 0,
            str1: Some(rule),
        };
        syntax_error::wrap(ruby, class, &error)
    };
    match build() {
        Ok(exception) => Error::from(exception),
        Err(error) => error,
    }
}

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let class = ruby.get_inner(&XPATH_CONTEXT);
    class.undef_default_alloc_func();
    class.define_singleton_method("new", function!(new, 1))?;
    class.define_method("node=", method!(XPathContext::set_node, 1))?;
    class.define_method("register_ns", method!(XPathContext::register_ns, 2))?;
    class.define_method(
        "register_variable",
        method!(XPathContext::register_variable, 2),
    )?;
    class.define_method("evaluate", method!(XPathContext::evaluate, -1))?;
    Ok(())
}
