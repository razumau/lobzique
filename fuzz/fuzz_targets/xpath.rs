#![no_main]

//! Parsing any bytes as XPath must return a query or an error, and
//! evaluating a query must find each node once, in document order. The
//! input is the expression, a NUL, and markup to search.
//!
//! Nested predicates cost the document's size to the power of their depth,
//! as in libxml2, so only shallow expressions on small documents are
//! evaluated. A `following` or `preceding` step from many nodes costs about
//! the square of the document's size, so the longer the path, the smaller
//! the document.

use libfuzzer_sys::fuzz_target;
use lobzique_core::dom::{Document, NodeId};
use lobzique_core::html5::{parse_document, Options};
use lobzique_core::xpath::{evaluate, parse, Outcome};
use std::collections::HashMap;

fuzz_target!(|data: &[u8]| {
    let (expression, markup) = match data.iter().position(|&b| b == 0) {
        Some(nul) => (&data[..nul], &data[nul + 1..]),
        None => (data, &b""[..]),
    };
    let Ok(query) = parse(expression) else {
        return;
    };
    let slashes = expression.iter().filter(|&&b| b == b'/').count();
    if markup.len() > 4096 || markup.len() * slashes > 8192 || nesting(expression) > 3 {
        return;
    }
    let options = Options {
        max_tree_depth: Some(400),
        max_attributes: Some(400),
        ..Options::default()
    };
    let Ok((document, _)) = parse_document(markup, &options) else {
        return;
    };
    let namespaces = HashMap::from([("svg".to_string(), "http://www.w3.org/2000/svg".to_string())]);
    let contexts = std::iter::once(NodeId::DOCUMENT)
        .chain(document.descendants(NodeId::DOCUMENT).step_by(7).take(4));
    for context in contexts {
        match evaluate(&document, &query, context, &namespaces, &HashMap::new()) {
            Ok(Outcome::Nodes(nodes)) => check_order(&document, &nodes),
            Ok(Outcome::Calls(_)) => panic!("XPath strings make no custom calls"),
            Err(_) => {}
        }
    }
});

/// How deep the expression's brackets go.
fn nesting(expression: &[u8]) -> usize {
    let (mut depth, mut deepest) = (0usize, 0);
    for b in expression {
        match b {
            b'[' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

fn check_order(document: &Document, nodes: &[NodeId]) {
    for pair in nodes.windows(2) {
        let order = document.document_order(pair[0], pair[1]);
        assert_eq!(order, Some(std::cmp::Ordering::Less), "{pair:?}");
    }
}
