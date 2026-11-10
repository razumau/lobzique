//! `Html5ever`, Nokogiri's `Gumbo` (`ext/nokogiri/gumbo.c`): the HTML5
//! parser's entry points behind `HTML5::Document.parse` and
//! `HTML5::DocumentFragment#initialize`, and the parse behind
//! `Node#in_context`.

use crate::support::{
    c_string, call_initialize, check_interrupts, check_string, string_bytes, without_gvl,
    Resumable,
};
use crate::xml::document::{self, wrap_document};
use crate::xml::node::{utf8_string, Node};
use crate::xml::node_set;
use crate::xml::syntax_error::{self, XmlError};
use crate::{HTML5EVER, XML};
use lobzique_core::html5::{
    self, Context, ContextError, ContextNamespace, DocumentParser, FragmentParser, LimitError,
    Options, ParseError, QuirksMode,
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
    let input_length = input.len();
    let klass = RClass::try_convert(klass)?;

    let mut parser = DocumentParser::new(input, &options);
    run(&mut parser, input_length)?;
    let (arena, parsed) = parser.finish().map_err(|error| limit_error(ruby, error))?;
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
    let context = context_for(ruby, context_arg, document)?;
    let input = input_bytes(ruby, input)?;
    let input_length = input.len();
    let target = Obj::<Node>::try_convert(fragment)?;

    let mut parser = FragmentParser::new(input, &context, &options);
    run(&mut parser, input_length)?;
    let nodes = parser.finish().map_err(|error| limit_error(ruby, error))?;
    // Nokogiri builds in the document `fragment.document` names; the nodes
    // can only go into the arena the fragment itself is in, which another
    // thread may have changed during the parse
    let parsed = target.with_arena_mut(ruby, move |arena, id| nodes.append_to(arena, id))?;
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
    let input_length = input.len();
    let document_object = Obj::<Node>::try_convert(node.document(ruby)?)?;
    let parsed = !document_object
        .ivar_get::<_, Value>("@quirks_mode")?
        .is_nil();
    // Parsed as the serializer will write it, as for fragments
    let noscript_as_text = document_object
        .ivar_get::<_, Value>("@parse_noscript_content_as_text")?
        .to_bool();
    let options = Options {
        // libxml2 keeps at most 100 errors a parse
        max_errors: Some(100),
        parse_noscript_content_as_text: noscript_as_text,
        ..default_options(ruby)?
    };

    let context = node.with_arena(ruby, |arena, id| Context::for_node(arena, id, parsed))?;
    let mut parser = FragmentParser::new(input, &context, &options);
    run(&mut parser, input_length)?;
    let nodes = parser.finish().map_err(|error| limit_error(ruby, error))?;
    // The node's document now, which another thread may have changed
    // during the parse
    let document = node.document(ruby)?;
    let (ids, parsed) = document::with_arena_mut(ruby, document, move |arena| nodes.import(arena))?;
    let errors = Obj::<Node>::try_convert(document)?.ivar_get::<_, RArray>("@errors")?;
    errors.concat(syntax_errors(ruby, parsed.errors, ruby.qnil().as_value())?)?;
    node_set::wrap(ruby, document, ids)
}

/// Inputs from this size (about 40 ms of parsing) are parsed without the
/// GVL. Getting it back from a thread running Ruby can take that thread's
/// whole time slice, 100 ms, which would more than double a shorter parse.
const WITHOUT_GVL_FROM: usize = 1 << 20;

/// Parses the input up to the end, so the parser's `finish` only ends the
/// tree. Interrupts are handled between steps, as between Ruby calls.
fn run(parser: &mut impl Resumable, input_length: usize) -> Result<(), Error> {
    if input_length >= WITHOUT_GVL_FROM {
        return without_gvl(parser);
    }
    while parser.step() {
        check_interrupts()?;
    }
    Ok(())
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

/// gumbo.c walks a node context through Ruby calls; lobzique reads the
/// node's arena, so the context must be a lobzique node.
fn context_for(ruby: &Ruby, context: Value, document: Value) -> Result<Context, Error> {
    if context.is_nil() {
        return Ok(Context::new(ContextNamespace::Html, "body"));
    }
    if RString::from_value(context).is_some() {
        return named_context(ruby, &c_string(context)?);
    }
    let node = Obj::<Node>::try_convert(context)?;
    let context = node
        .with_arena(ruby, Context::of_node)?
        .map_err(|error| context_error(ruby, error))?;
    Ok(context.with_quirks_mode(quirks_mode_for(ruby, document)?))
}

fn context_error(ruby: &Ruby, error: ContextError) -> Error {
    match error {
        ContextError::NoName => Error::new(
            ruby.exception_type_error(),
            "wrong argument type nil (expected String)",
        ),
        ContextError::UnknownNamespace(href) => Error::new(
            ruby.exception_arg_error(),
            format!("Unexpected namespace URI \"{href}\""),
        ),
    }
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

/// For a node context: no quirks unless the fragment's document was
/// parsed, then the quirks mode of its doctype.
fn quirks_mode_for(ruby: &Ruby, document: Value) -> Result<QuirksMode, Error> {
    let parsed = !Obj::<Node>::try_convert(document)?
        .ivar_get::<_, Value>("@quirks_mode")?
        .is_nil();
    if !parsed {
        return Ok(QuirksMode::NoQuirks);
    }
    document::with_arena(ruby, document, html5::doctype_quirks_mode)
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
