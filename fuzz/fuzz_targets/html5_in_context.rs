#![no_main]

//! Parsing pieces of input in context, one after another in one document,
//! must leave a consistent tree: each parse returns unlinked nodes and adds
//! nothing to the document but them, and a parse that fails adds nothing.
//! The first byte picks the context; NUL bytes split the rest into pieces.

use libfuzzer_sys::fuzz_target;
use lobzique_core::dom::{Document, DocumentKind};
use std::iter;
use lobzique_core::html5::{parse_in_context, serialize, Context, ContextNamespace, Options};

const CONTEXTS: &[(ContextNamespace, &str)] = &[
    (ContextNamespace::Html, "body"),
    (ContextNamespace::Html, "tr"),
    (ContextNamespace::Html, "select"),
    (ContextNamespace::Html, "html"),
    (ContextNamespace::Svg, "foreignObject"),
    (ContextNamespace::MathMl, "annotation-xml"),
];

fuzz_target!(|data: &[u8]| {
    let Some((&selector, input)) = data.split_first() else {
        return;
    };
    let options = Options {
        max_attributes: Some(400),
        max_errors: Some(100),
        max_tree_depth: Some(400),
        parse_noscript_content_as_text: false,
    };
    let (namespace, name) = CONTEXTS[usize::from(selector) % CONTEXTS.len()];
    let context = Context::new(namespace, name)
        .with_form_ancestor(selector & 0x80 != 0)
        .with_encoding((selector & 0x40 != 0).then(|| b"text/html".to_vec()));
    let kind = if selector & 0x20 != 0 {
        DocumentKind::Xml
    } else {
        DocumentKind::Html
    };
    let mut document = Document::new(kind);
    for piece in input.split(|&byte| byte == 0) {
        let before = document.node_ids().len();
        let Ok((nodes, _)) = parse_in_context(&mut document, piece, &context, &options) else {
            assert_eq!(document.node_ids().len(), before);
            continue;
        };
        let added: usize = nodes
            .iter()
            .flat_map(|&node| iter::once(node).chain(document.descendants(node)))
            .map(|node| 1 + document.attributes(node).len())
            .sum();
        assert_eq!(document.node_ids().len(), before + added);
        for &node in &nodes {
            assert_eq!(document.parent(node), None);
            assert_eq!(document.previous_sibling(node), None);
            assert_eq!(document.next_sibling(node), None);
            serialize(&document, node, false);
        }
        if let Err(violation) = document.check_invariants() {
            panic!("{violation}");
        }
    }
});
