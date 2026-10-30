//! `Html5ever`, Nokogiri's `Gumbo` (`ext/nokogiri/gumbo.c`): the HTML5
//! parser's entry points behind `HTML5::Document.parse` and
//! `HTML5::DocumentFragment#initialize`, and the parse behind
//! `Node#in_context`.

use crate::support::{c_string, c_string_with_length, call_initialize, check_string, string_bytes};
use crate::xml::document::{self, wrap_document};
use crate::xml::node::{utf8_string, Node};
use crate::xml::node_set;
use crate::xml::syntax_error::{self, XmlError};
use crate::{HTML5EVER, XML};
use lobzique_core::html5::{
    self, Context, ContextNamespace, LimitError, Options, ParseError, QuirksMode,
};
use magnus::{
    function,
    prelude::*,
    scan_args::{get_kwargs, scan_args},
    typed_data::Obj,
    Error, RArray, RClass, RHash, RString, Ruby, Value,
};
use std::sync::OnceLock;

/// `Html5ever.parse(input, url, klass, **options)`: a document of `klass`,
/// initialized with no arguments, then given `@url`, `@quirks_mode` and,
/// if there were any, `@errors`.
fn parse(ruby: &Ruby, args: &[Value]) -> Result<Value, Error> {
    let args = scan_args::<(Value, Value, Value), (), (), (), RHash, ()>(args)?;
    let (input, url, klass) = args.required;
    let options = options(args.keywords)?;
    let input = input_bytes(ruby, input)?;
    let klass = RClass::try_convert(klass)?;

    let (arena, parsed) =
        html5::parse_document(&input, &options).map_err(|error| limit_error(ruby, error))?;
    let document = wrap_document(ruby, klass, arena)?;
    call_initialize(document, &[])?;
    let object = Obj::<Node>::try_convert(document)?;
    object.ivar_set("@url", url)?;
    object.ivar_set("@quirks_mode", parsed.quirks_mode as u8)?;
    add_errors(ruby, object, parsed.errors, url)?;
    Ok(document)
}

/// `Html5ever.fragment(fragment, input, context, **options)`: parses
/// `input` in `context` (nil, a name such as `"svg:path"`, or a node) and
/// appends the result to `fragment`.
fn fragment(ruby: &Ruby, args: &[Value]) -> Result<(), Error> {
    let args = scan_args::<(Value, Value, Value), (), (), (), RHash, ()>(args)?;
    let (fragment, input, context_arg) = args.required;
    let options = options(args.keywords)?;
    let document: Value = fragment.funcall("document", ())?;
    let context =
        context_for(ruby, context_arg)?.with_quirks_mode(quirks_mode_for(document, context_arg)?);
    let input = input_bytes(ruby, input)?;

    // Nokogiri builds in the document `fragment.document` names; the nodes
    // can only go into the arena the fragment itself is in
    let target = Obj::<Node>::try_convert(fragment)?;
    let id = target.id(ruby)?;
    let parsed = document::with_arena_mut(ruby, target.document(ruby)?, |arena| {
        html5::parse_fragment(arena, id, &input, &context, &options)
    })?
    .map_err(|error| limit_error(ruby, error))?;
    target.ivar_set("@quirks_mode", parsed.quirks_mode as u8)?;
    add_errors(
        ruby,
        target,
        parsed.errors,
        utf8_string(ruby, "#fragment").as_value(),
    )
}

/// `Node#in_context`'s parse: `input` as a fragment in the context
/// `Context::for_node` finds for `node`. The top-level nodes come back
/// unlinked, and the parse errors are added to the document's `@errors`.
pub fn in_context(ruby: &Ruby, node: &Node, input: Value) -> Result<Value, Error> {
    let input = input_bytes(ruby, input)?;
    let document = node.document(ruby)?;
    let document_object = Obj::<Node>::try_convert(document)?;
    let parsed = !document_object
        .ivar_get::<_, Value>("@quirks_mode")?
        .is_nil();
    let options = Options {
        // libxml2 keeps at most 100 errors a parse
        max_errors: Some(100),
        ..default_options(ruby)?
    };

    let (ids, parsed) = node
        .with_arena_mut(ruby, |arena, id| {
            let context = Context::for_node(arena, id, parsed);
            html5::parse_in_context(arena, &input, &context, &options)
        })?
        .map_err(|error| limit_error(ruby, error))?;
    let errors = document_object.ivar_get::<_, RArray>("@errors")?;
    errors.concat(syntax_errors(ruby, parsed.errors, ruby.qnil().as_value())?)?;
    node_set::wrap(ruby, document, ids)
}

/// The `Html5ever::DEFAULT_*` limits, which are constant once loaded.
fn default_options(ruby: &Ruby) -> Result<Options, Error> {
    static DEFAULTS: OnceLock<Options> = OnceLock::new();
    if let Some(options) = DEFAULTS.get() {
        return Ok(options.clone());
    }
    let module = ruby.get_inner(&HTML5EVER);
    let default = |name: &str| module.const_get::<_, i32>(name).map(limit);
    let options = Options {
        max_attributes: default("DEFAULT_MAX_ATTRIBUTES")?,
        max_errors: default("DEFAULT_MAX_ERRORS")?,
        max_tree_depth: default("DEFAULT_MAX_TREE_DEPTH")?,
        parse_noscript_content_as_text: false,
    };
    Ok(DEFAULTS.get_or_init(|| options).clone())
}

/// Gumbo's options: three required keywords and one optional one.
fn options(keywords: RHash) -> Result<Options, Error> {
    let keywords = get_kwargs::<_, (i32, i32, i32), (Option<Value>,), ()>(
        keywords,
        &["max_attributes", "max_errors", "max_tree_depth"],
        &["parse_noscript_content_as_text"],
    )?;
    let (max_attributes, max_errors, max_tree_depth) = keywords.required;
    let (noscript_as_text,) = keywords.optional;
    Ok(Options {
        max_attributes: limit(max_attributes),
        max_errors: limit(max_errors),
        max_tree_depth: limit(max_tree_depth),
        parse_noscript_content_as_text: noscript_as_text.is_some_and(|value| value.to_bool()),
    })
}

/// A negative limit is no limit.
fn limit(value: i32) -> Option<usize> {
    usize::try_from(value).ok()
}

/// The whole string, zero bytes included, as gumbo reads it.
fn input_bytes(ruby: &Ruby, input: Value) -> Result<Vec<u8>, Error> {
    check_string(ruby, input)?;
    let input = RString::from_value(input).expect("checked to be a String");
    Ok(string_bytes(input))
}

fn limit_error(ruby: &Ruby, error: LimitError) -> Error {
    Error::new(ruby.exception_arg_error(), error.to_string())
}

fn context_for(ruby: &Ruby, context: Value) -> Result<Context, Error> {
    if context.is_nil() {
        return Ok(Context::new(ContextNamespace::Html, "body"));
    }
    if RString::from_value(context).is_some() {
        return named_context(ruby, &c_string(context)?);
    }

    let name: Value = context.funcall("name", ())?;
    check_string(ruby, name)?;
    let name = String::from_utf8_lossy(&c_string(name)?).into_owned();
    let namespace = namespace_of(ruby, context)?.ok_or_else(|| {
        let href = context
            .funcall::<_, _, Value>("namespace", ())
            .and_then(|ns| ns.funcall::<_, _, Value>("href", ()))
            .and_then(c_string_with_length)
            .unwrap_or_default();
        let href = String::from_utf8_lossy(&href);
        let message = format!("Unexpected namespace URI \"{href}\"");
        Error::new(ruby.exception_arg_error(), message)
    })?;

    let mut encoding = None;
    if namespace == ContextNamespace::MathMl && name.eq_ignore_ascii_case("annotation-xml") {
        let value: Value = context.funcall("[]", ("encoding",))?;
        if value.to_bool() {
            check_string(ruby, value)?;
            encoding = Some(c_string(value)?);
        }
    }

    Ok(Context::new(namespace, &name)
        .with_encoding(encoding)
        .with_form_ancestor(has_form_ancestor(ruby, context)?))
}

/// Whether `node` or an element above it is an HTML `form`.
fn has_form_ancestor(ruby: &Ruby, node: Value) -> Result<bool, Error> {
    let mut node = node;
    while !node.is_nil() {
        if node.funcall::<_, _, Value>("element?", ())?.to_bool() {
            let name = c_string_with_length(node.funcall("name", ())?)?;
            if name.eq_ignore_ascii_case(b"form")
                && namespace_of(ruby, node)? == Some(ContextNamespace::Html)
            {
                return Ok(true);
            }
        }
        node = if node.respond_to("parent", false)? {
            node.funcall("parent", ())?
        } else {
            ruby.qnil().as_value()
        };
    }
    Ok(false)
}

/// A context given by name: `"tr"`, `"svg:path"`, or `"svg"` and `"math"`,
/// which name their own namespace.
fn named_context(ruby: &Ruby, tag: &[u8]) -> Result<Context, Error> {
    let tag = String::from_utf8_lossy(tag);
    let (namespace, name) = match tag.split_once(':') {
        Some((prefix, name)) => {
            let namespace = match prefix.to_ascii_lowercase().as_str() {
                "svg" => ContextNamespace::Svg,
                "html" => ContextNamespace::Html,
                "math" => ContextNamespace::MathMl,
                _ => {
                    // gumbo.c's "%*s" pads to the prefix's length, so the
                    // whole name is shown
                    let message = format!("Invalid context namespace '{tag}'");
                    return Err(Error::new(ruby.exception_arg_error(), message));
                }
            };
            (namespace, name)
        }
        None if tag.eq_ignore_ascii_case("svg") => (ContextNamespace::Svg, &*tag),
        None if tag.eq_ignore_ascii_case("math") => (ContextNamespace::MathMl, &*tag),
        None => (ContextNamespace::Html, &*tag),
    };
    let form = namespace == ContextNamespace::Html && name.eq_ignore_ascii_case("form");
    Ok(Context::new(namespace, name).with_form_ancestor(form))
}

/// gumbo.c's `lookup_namespace`, through `node.namespace.href`; `None` for
/// a namespace the parser has no rules for.
fn namespace_of(ruby: &Ruby, node: Value) -> Result<Option<ContextNamespace>, Error> {
    let namespace: Value = node.funcall("namespace", ())?;
    if namespace.is_nil() {
        return Ok(Some(ContextNamespace::Html));
    }
    let href: Value = namespace.funcall("href", ())?;
    check_string(ruby, href)?;
    Ok(match &*c_string_with_length(href)? {
        b"http://www.w3.org/1999/xhtml" => Some(ContextNamespace::Html),
        b"http://www.w3.org/1998/Math/MathML" => Some(ContextNamespace::MathMl),
        b"http://www.w3.org/2000/svg" => Some(ContextNamespace::Svg),
        _ => None,
    })
}

/// No quirks unless the context is a node of a parsed document; then the
/// quirks mode its doctype selects, or quirks if it has none.
fn quirks_mode_for(document: Value, context: Value) -> Result<QuirksMode, Error> {
    let doctype: Value = document.funcall("internal_subset", ())?;
    let document_quirks_mode =
        Obj::<Node>::try_convert(document)?.ivar_get::<_, Value>("@quirks_mode")?;
    if context.is_nil() || RString::from_value(context).is_some() || document_quirks_mode.is_nil() {
        return Ok(QuirksMode::NoQuirks);
    }
    if doctype.is_nil() {
        return Ok(QuirksMode::Quirks);
    }
    let field = |name: &str| -> Result<Option<Vec<u8>>, Error> {
        let value: Value = doctype.funcall(name, ())?;
        (!value.is_nil()).then(|| c_string(value)).transpose()
    };
    let (name, public_id, system_id) = (field("name")?, field("external_id")?, field("system_id")?);
    Ok(html5::quirks_mode_for_doctype(
        name.as_deref(),
        public_id.as_deref(),
        system_id.as_deref(),
    ))
}

/// gumbo.c's `add_errors`: one `XML::SyntaxError` per parse error, set
/// only if there were any.
fn add_errors(
    ruby: &Ruby,
    target: Obj<Node>,
    errors: Vec<ParseError>,
    file: Value,
) -> Result<(), Error> {
    if errors.is_empty() {
        return Ok(());
    }
    target.ivar_set("@errors", syntax_errors(ruby, errors, file)?)
}

/// html5ever's errors have neither gumbo's error code, which gumbo.c puts
/// in `str1`, nor a column.
fn syntax_errors(ruby: &Ruby, errors: Vec<ParseError>, file: Value) -> Result<RArray, Error> {
    let class: RClass = ruby.get_inner(&XML).const_get("SyntaxError")?;
    let array = ruby.ary_new_capa(errors.len());
    for error in errors {
        let error = XmlError {
            domain: 1, // XML_FROM_PARSER
            code: 1,   // XML_ERR_INTERNAL_ERROR
            message: &error.message,
            level: 2, // XML_ERR_ERROR
            file,
            line: error.line,
            str1: None,
        };
        array.push(syntax_error::wrap(ruby, class, &error)?)?;
    }
    Ok(array)
}

pub fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.get_inner(&HTML5EVER);
    module.define_singleton_method("parse", function!(parse, -1))?;
    module.define_singleton_method("fragment", function!(fragment, -1))?;
    Ok(())
}
