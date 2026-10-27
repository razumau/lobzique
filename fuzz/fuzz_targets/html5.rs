#![no_main]

//! Parsing any input, as a document or as a fragment in one of several
//! contexts, must leave a consistent tree that serializes without panicking.
//! The first byte picks how the rest is parsed.

use libfuzzer_sys::fuzz_target;
use lobzique_core::dom::{Document, DocumentKind, NodeId};
use lobzique_core::html5::{
    parse_document, parse_fragment, serialize, Context, ContextNamespace, Options, QuirksMode,
};

const CONTEXTS: &[(ContextNamespace, &str)] = &[
    (ContextNamespace::Html, "body"),
    (ContextNamespace::Html, "table"),
    (ContextNamespace::Html, "select"),
    (ContextNamespace::Html, "template"),
    (ContextNamespace::Html, "textarea"),
    (ContextNamespace::Svg, "svg"),
    (ContextNamespace::MathMl, "annotation-xml"),
];

fuzz_target!(|data: &[u8]| {
    let Some((&selector, input)) = data.split_first() else {
        return;
    };
    // Nokogiri's defaults; without a depth limit, deep input is quadratic
    let options = Options {
        max_attributes: Some(400),
        max_errors: Some(usize::from(selector & 1)),
        max_tree_depth: Some(400),
        parse_noscript_content_as_text: selector & 2 != 0,
    };
    let (document, root) = match usize::from(selector >> 2) % (CONTEXTS.len() + 1) {
        0 => match parse_document(input, &options) {
            Ok((document, _)) => (document, NodeId::DOCUMENT),
            Err(_) => return,
        },
        index => {
            let (namespace, name) = CONTEXTS[index - 1];
            let context = Context::new(namespace, name)
                .with_form_ancestor(selector & 0x80 != 0)
                .with_encoding((selector & 0x40 != 0).then(|| b"text/html".to_vec()))
                .with_quirks_mode(if selector & 0x20 != 0 {
                    QuirksMode::Quirks
                } else {
                    QuirksMode::NoQuirks
                });
            let mut document = Document::new(DocumentKind::Html);
            let fragment = document.create_fragment();
            if parse_fragment(&mut document, fragment, input, &context, &options).is_err() {
                return;
            }
            (document, fragment)
        }
    };
    if let Err(violation) = document.check_invariants() {
        panic!("{violation}");
    }
    serialize(&document, root, selector & 0x10 != 0);
});
