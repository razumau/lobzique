#![no_main]

//! Parsing any bytes as XPath must return a query or an error, and
//! evaluating a query must return a value, with each node of a node set
//! once and in document order, or an error. The input is the expression, a
//! NUL, and markup to search. Custom calls get answers of every type.
//!
//! Nested predicates cost the document's size to the power of their depth,
//! as in libxml2, so only shallow expressions on small documents are
//! evaluated. A `following` or `preceding` step from many nodes costs about
//! the square of the document's size, and so do the string functions on the
//! document's string value, so the more steps and calls, the smaller the
//! document.

use libfuzzer_sys::fuzz_target;
use lobzique_core::dom::{Document, NodeId};
use lobzique_core::html5::{parse_document, Options};
use lobzique_core::xpath::{evaluate, parse, Error, Outcome, Value};
use std::borrow::Cow;
use std::collections::HashMap;

fuzz_target!(|data: &[u8]| {
    let (expression, markup) = match data.iter().position(|&b| b == 0) {
        Some(nul) => (&data[..nul], &data[nul + 1..]),
        None => (data, &b""[..]),
    };
    let Ok(query) = parse(expression) else {
        return;
    };
    let steps_and_calls = expression
        .iter()
        .filter(|&&b| matches!(b, b'/' | b'('))
        .count();
    if markup.len() > 4096 || markup.len() * steps_and_calls > 8192 || nesting(expression) > 3 {
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
    let variables = HashMap::from([("v".to_string(), b"1".to_vec())]);
    let contexts = std::iter::once(NodeId::DOCUMENT)
        .chain(document.descendants(NodeId::DOCUMENT).step_by(7).take(4));
    for context in contexts {
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
                Outcome::Value(_) => break,
                Outcome::Calls(calls) => {
                    assert!(round < 10_000, "custom calls never settle");
                    for call in calls {
                        let answer = match (call.node.index() + call.site) % 5 {
                            0 => Ok(Value::Boolean(true)),
                            1 => Ok(Value::Number(1.0)),
                            2 => Ok(Value::String(Cow::Borrowed(b"x"))),
                            3 => Ok(Value::NodeSet(vec![call.node])),
                            _ => Err(Error::UnregisteredFunction),
                        };
                        answers.insert((call.site, call.node), answer);
                    }
                }
            }
        }
    }
});

/// How deep the expression's brackets go.
fn nesting(expression: &[u8]) -> usize {
    let (mut depth, mut deepest) = (0usize, 0);
    for b in expression {
        match b {
            b'[' | b'(' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b']' | b')' => depth = depth.saturating_sub(1),
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
