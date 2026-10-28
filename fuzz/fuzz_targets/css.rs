#![no_main]

//! Compiling any selector must return a selector or a syntax error, and
//! evaluating it must find each node once, in document order, answering
//! custom calls until none are left. The input is the selector, a NUL, and
//! markup to search; the first byte picks the options.
//!
//! Nested `:has(…)` costs the document's size to the power of its depth, as
//! in Nokogiri, so only shallow selectors on small documents are evaluated.

use libfuzzer_sys::fuzz_target;
use lobzique_core::css::{compile, Options, Prefix};
use lobzique_core::dom::{Document, NodeId};
use lobzique_core::html5::{parse_document, Options as ParseOptions};
use lobzique_core::xpath::{evaluate, Outcome, Value};
use std::borrow::Cow;
use std::collections::HashMap;

const PREFIXES: &[&[Prefix]] = &[
    &[Prefix::Global],
    &[Prefix::Subtree],
    &[Prefix::Subtree, Prefix::Self_],
    &[Prefix::Current],
];

fuzz_target!(|data: &[u8]| {
    let Some((&flags, rest)) = data.split_first() else {
        return;
    };
    let (selector, markup) = match rest.iter().position(|&b| b == 0) {
        Some(nul) => (&rest[..nul], &rest[nul + 1..]),
        None => (rest, &b""[..]),
    };
    let Ok(selector) = std::str::from_utf8(selector) else {
        return;
    };
    let options = Options {
        html: flags & 1 != 0,
        builtins: flags & 2 != 0,
        default_namespace: flags & 4 != 0,
        prefixes: PREFIXES[usize::from(flags >> 3) % PREFIXES.len()].to_vec(),
        first_only: flags & 0x80 != 0,
    };
    let Ok(compiled) = compile(selector, &options) else {
        return;
    };
    let Ok(query) = compiled.query else {
        return;
    };
    if markup.len() > 4096 || nesting(selector) > 3 {
        return;
    }
    let parse_options = ParseOptions {
        max_tree_depth: Some(400),
        max_attributes: Some(400),
        ..ParseOptions::default()
    };
    let Ok((document, _)) = parse_document(markup, &parse_options) else {
        return;
    };
    let namespaces = HashMap::from([
        ("svg".to_string(), "http://www.w3.org/2000/svg".to_string()),
        (
            "xmlns".to_string(),
            "http://www.w3.org/1999/xhtml".to_string(),
        ),
    ]);
    let contexts = std::iter::once(NodeId::DOCUMENT)
        .chain(document.descendants(NodeId::DOCUMENT).step_by(7).take(4));
    for context in contexts {
        let mut answers = HashMap::new();
        for round in 0.. {
            let outcome = match evaluate(&document, &query, context, &namespaces, &answers) {
                Ok(outcome) => outcome,
                Err(_) => break,
            };
            match outcome {
                Outcome::Nodes(nodes) => {
                    check_order(&document, &nodes);
                    break;
                }
                Outcome::Calls(calls) => {
                    assert!(round < 10_000, "custom calls never settle");
                    for call in calls {
                        let answer = match (call.node.index() + call.site) % 4 {
                            0 => Value::Boolean(true),
                            1 => Value::Number(1.0),
                            2 => Value::String(Cow::Borrowed(b"x")),
                            _ => Value::NodeSet(vec![call.node]),
                        };
                        answers.insert((call.site, call.node), answer);
                    }
                }
            }
        }
    }
});

/// How deep the selector's parentheses go.
fn nesting(selector: &str) -> usize {
    let (mut depth, mut deepest) = (0usize, 0);
    for c in selector.chars() {
        match c {
            '(' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            ')' => depth = depth.saturating_sub(1),
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
