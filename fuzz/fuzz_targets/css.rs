#![no_main]

//! A selector must compile to a selector or a syntax error. Evaluation must
//! find each node once, in document order, and answer custom calls until none
//! remain. The input is the selector, a NUL, and markup to search. The first
//! byte picks the options.
//!
//! Writing the selector as XPath (`CSS.xpath_for`) must return strings
//! or a syntax error. With one prefix, the strings parsed as XPath must find
//! what the compiled selector finds, and fail where it fails.
//!
//! Nested `:has(…)` costs the size of the document to the power of its depth,
//! as in Nokogiri. Selectors nested deeper than SHALLOW are evaluated only
//! on tiny documents, whose node count to that power stays within BUDGET.

use libfuzzer_sys::fuzz_target;
use lobzique_core::css::{compile, xpath_for, Options, Prefix, Visitor};
use lobzique_core::dom::{Document, NodeId};
use lobzique_core::html5::{parse_document, Options as ParseOptions};
use lobzique_core::xpath::{
    self, evaluate, Outcome, Query, Value, NOKOGIRI_BUILTIN_URI, NOKOGIRI_URI,
};
use std::borrow::Cow;
use std::collections::HashMap;

const SHALLOW: usize = 3;
const TINY: usize = 64;
const BUDGET: u64 = 1 << 20;

const PREFIXES: &[(&[Prefix], &str)] = &[
    (&[Prefix::Global], "//"),
    (&[Prefix::Subtree], ".//"),
    (&[Prefix::Subtree, Prefix::Self_], "self::"),
    (&[Prefix::Current], "./"),
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
    let (prefixes, prefix) = PREFIXES[usize::from(flags >> 3) % PREFIXES.len()];
    let options = Options {
        html: flags & 1 != 0,
        builtins: flags & 2 != 0,
        default_namespace: flags & 4 != 0,
        prefixes: prefixes.to_vec(),
        first_only: flags & 0x80 != 0,
    };
    let visitor = Visitor {
        builtins: options.builtins,
        html5: options.html,
        default_namespace: options.default_namespace,
        prefix: prefix.into(),
    };
    let written = xpath_for(selector, &visitor);
    let Ok(compiled) = compile(selector, &options) else {
        return;
    };
    // Without builtins the visitor writes HTML names as predicates.
    // This changes which positions count.
    let comparable = prefixes.len() == 1 && !options.first_only && (options.builtins || !options.html);
    let written = match written {
        Ok(paths) if comparable => Some(xpath::parse(paths.join("|").as_bytes())),
        _ => None,
    };
    let query = match (compiled.query, &written) {
        (Ok(query), _) => query,
        (Err(_), Some(Ok(_))) => panic!("the compiled selector is invalid, its XPath isn't"),
        (Err(_), _) => return,
    };
    let written: Option<Query> = written.map(|parsed| match parsed {
        Ok(parsed) => parsed,
        Err(error) => panic!("the selector compiles, its XPath doesn't parse: {error:?}"),
    });
    let depth = nesting(selector);
    if markup.len() > if depth > SHALLOW { TINY } else { 4096 } {
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
    if depth > SHALLOW && !within_budget(&document, depth) {
        return;
    }
    let namespaces = HashMap::from([
        ("svg".to_string(), "http://www.w3.org/2000/svg".to_string()),
        (
            "xmlns".to_string(),
            "http://www.w3.org/1999/xhtml".to_string(),
        ),
        ("nokogiri".to_string(), NOKOGIRI_URI.to_string()),
        (
            "nokogiri-builtin".to_string(),
            NOKOGIRI_BUILTIN_URI.to_string(),
        ),
    ]);
    let contexts = std::iter::once(NodeId::DOCUMENT)
        .chain(document.descendants(NodeId::DOCUMENT).step_by(7).take(4));
    let variables = HashMap::new();
    for context in contexts {
        if let Some(written) = &written {
            let none = HashMap::new();
            let found = [&query, written]
                .map(|query| evaluate(&document, query, context, &namespaces, &variables, &none));
            match found {
                [Ok(Outcome::Value(compiled)), Ok(Outcome::Value(written))] => {
                    assert_eq!(compiled, written, "the XPath finds something else");
                }
                [Ok(Outcome::Calls(_)), Ok(Outcome::Calls(_))] | [Err(_), Err(_)] => {}
                [compiled, written] => panic!("compiled: {compiled:?}, written: {written:?}"),
            }
        }
        let mut answers = HashMap::new();
        for round in 0.. {
            let outcome = match evaluate(
                &document,
                &query,
                context,
                &namespaces,
                &variables,
                &answers,
            ) {
                Ok(outcome) => outcome,
                Err(_) => break,
            };
            match outcome {
                Outcome::Value(Value::NodeSet(nodes)) => {
                    check_order(&document, &nodes);
                    break;
                }
                Outcome::Value(value) => panic!("selectors find node sets, not {value:?}"),
                Outcome::Calls(calls) => {
                    assert!(round < 10_000, "custom calls never settle");
                    for call in calls {
                        let answer = match (call.node.index() + call.site) % 4 {
                            0 => Value::Boolean(true),
                            1 => Value::Number(1.0),
                            2 => Value::String(Cow::Borrowed(b"x")),
                            _ => Value::NodeSet(vec![call.node]),
                        };
                        answers.insert(call, Ok(answer));
                    }
                }
            }
        }
    }
});

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

fn within_budget(document: &Document, depth: usize) -> bool {
    let nodes = document.node_ids().len() as u64;
    u32::try_from(depth)
        .ok()
        .and_then(|depth| nodes.checked_pow(depth))
        .is_some_and(|cost| cost <= BUDGET)
}

fn check_order(document: &Document, nodes: &[NodeId]) {
    for pair in nodes.windows(2) {
        let order = document.document_order(pair[0], pair[1]);
        assert_eq!(order, Some(std::cmp::Ordering::Less), "{pair:?}");
    }
}
