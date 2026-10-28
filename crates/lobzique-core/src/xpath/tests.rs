use super::eval::{number_to_string, string_to_number};
use super::*;
use crate::dom::{Document, NodeId, NodeKind};
use crate::html5::{parse_document, Options};
use std::collections::HashMap;

fn parse(input: &str) -> Document {
    parse_document(input.as_bytes(), &Options::default())
        .unwrap()
        .0
}

fn element(document: &Document, id: &str) -> NodeId {
    document
        .descendants(NodeId::DOCUMENT)
        .find(|&node| document.attribute_value(node, b"id") == Some(id.as_bytes()))
        .unwrap()
}

/// The ids, or contents, of what `expr` finds from `context`.
fn run(document: &Document, context: NodeId, expr: Expr) -> Vec<String> {
    let query = Query::new(expr, Vec::new());
    let Ok(Outcome::Nodes(nodes)) =
        evaluate(document, &query, context, &HashMap::new(), &HashMap::new())
    else {
        panic!("expected nodes");
    };
    nodes
        .into_iter()
        .map(|node| {
            let bytes = match document.node_kind(node) {
                NodeKind::Element(_) => document
                    .attribute_value(node, b"id")
                    .unwrap_or(b"?")
                    .to_vec(),
                _ => document.content(node).unwrap_or_default(),
            };
            String::from_utf8(bytes).unwrap()
        })
        .collect()
}

fn step(axis: Axis, test: NodeTest, predicates: Vec<Expr>) -> Step {
    Step {
        axis,
        test,
        predicates,
    }
}

fn path(steps: Vec<Step>) -> Expr {
    Expr::Path(Path::relative(steps))
}

fn number(n: f64) -> Expr {
    Expr::Number(n)
}

/// Whether `predicate` holds at `context`, as `self::node()[predicate]`.
fn holds(document: &Document, context: NodeId, predicate: Expr) -> bool {
    let expr = path(vec![step(Axis::Self_, NodeTest::Node, vec![predicate])]);
    !run(document, context, expr).is_empty()
}

const SIBLINGS: &str =
    "<div id=d><p id=a><b id=ab></b></p><p id=b></p><p id=c><i id=ci></i></p></div>";

#[test]
fn reverse_axes_count_from_the_context_node() {
    let document = parse(SIBLINGS);
    let c = element(&document, "c");
    let first = |axis| path(vec![step(axis, NodeTest::Any, vec![number(1.0)])]);
    assert_eq!(run(&document, c, first(Axis::PrecedingSibling)), ["b"]);
    assert_eq!(run(&document, c, first(Axis::Ancestor)), ["d"]);
    assert_eq!(run(&document, c, first(Axis::Preceding)), ["b"]);
    // but the result is in document order
    let all = path(vec![step(Axis::Preceding, NodeTest::Any, Vec::new())]);
    // `?`: <head>, before <body>
    assert_eq!(run(&document, c, all), ["?", "a", "ab", "b"]);
    let following = path(vec![step(Axis::Following, NodeTest::Any, Vec::new())]);
    assert_eq!(
        run(&document, element(&document, "a"), following),
        ["b", "c", "ci"]
    );
}

#[test]
fn constant_positions_stop_the_axis_early() {
    let document = parse(SIBLINGS);
    let d = element(&document, "d");
    let children = |predicates| path(vec![step(Axis::Child, NodeTest::Any, predicates)]);
    assert_eq!(run(&document, d, children(vec![number(2.0)])), ["b"]);
    assert_eq!(run(&document, d, children(vec![number(4.0)])), [""; 0]);
    assert_eq!(run(&document, d, children(vec![number(0.0)])), [""; 0]);
    assert_eq!(run(&document, d, children(vec![number(1.5)])), [""; 0]);
    // the next predicate counts what the first one kept
    let last = Expr::compare(
        Comparison::Equal,
        Expr::Call(Function::Position, Vec::new()),
        Expr::Call(Function::Last, Vec::new()),
    );
    assert_eq!(run(&document, d, children(vec![number(2.0), last])), ["b"]);
    let from_c = |predicates| {
        path(vec![step(
            Axis::PrecedingSibling,
            NodeTest::Any,
            predicates,
        )])
    };
    assert_eq!(
        run(
            &document,
            element(&document, "c"),
            from_c(vec![number(2.0)])
        ),
        ["a"]
    );
}

#[test]
fn steps_from_several_nodes_merge_in_document_order() {
    let document = parse(SIBLINGS);
    let expr = path(vec![
        step(Axis::Descendant, NodeTest::Any, Vec::new()),
        step(Axis::Parent, NodeTest::Any, Vec::new()),
    ]);
    assert_eq!(
        run(&document, element(&document, "d"), expr),
        ["d", "a", "c"]
    );
    let union = Expr::Union(vec![
        Path::relative(vec![step(Axis::Child, NodeTest::Any, vec![number(3.0)])]),
        Path::relative(vec![step(Axis::Child, NodeTest::Any, vec![number(1.0)])]),
        Path::relative(vec![step(Axis::Child, NodeTest::Any, vec![number(3.0)])]),
    ]);
    assert_eq!(run(&document, element(&document, "d"), union), ["a", "c"]);
}

#[test]
fn comparisons_follow_xpath_1() {
    let document = parse("<p id=p class=' 2 '>x</p>");
    let p = element(&document, "p");
    let class = || {
        path(vec![step(
            Axis::Attribute,
            NodeTest::Name(QName {
                prefix: None,
                local: "class".into(),
            }),
            Vec::new(),
        )])
    };
    let missing = || {
        path(vec![step(
            Axis::Attribute,
            NodeTest::Name(QName {
                prefix: None,
                local: "x".into(),
            }),
            Vec::new(),
        )])
    };
    let literal = |s: &str| Expr::Literal(s.as_bytes().to_vec());
    // a node set compares as its nodes' string values, or as numbers
    assert!(holds(
        &document,
        p,
        Expr::compare(Comparison::Equal, class(), literal(" 2 "))
    ));
    assert!(holds(
        &document,
        p,
        Expr::compare(Comparison::Equal, class(), number(2.0))
    ));
    assert!(holds(
        &document,
        p,
        Expr::compare(Comparison::Less, class(), number(3.0))
    ));
    // an empty node set equals nothing and differs from nothing
    assert!(!holds(
        &document,
        p,
        Expr::compare(Comparison::Equal, missing(), literal(""))
    ));
    assert!(!holds(
        &document,
        p,
        Expr::compare(Comparison::NotEqual, missing(), literal(""))
    ));
    // but its string value is empty
    assert!(holds(
        &document,
        p,
        Expr::Call(Function::StartsWith, vec![missing(), literal("")])
    ));
    // against a boolean, a node set is its own boolean
    let falsehood = Expr::Call(Function::Not, vec![number(1.0)]);
    assert!(holds(
        &document,
        p,
        Expr::compare(Comparison::Equal, missing(), falsehood)
    ));
    assert!(holds(
        &document,
        p,
        Expr::compare(Comparison::Equal, literal("1"), number(1.0))
    ));
    let nan = Expr::arithmetic(Arithmetic::Modulo, number(1.0), number(0.0));
    assert!(!holds(
        &document,
        p,
        Expr::compare(Comparison::Equal, nan.clone(), nan)
    ));
}

#[test]
fn string_functions() {
    let document = parse("<p id=p class='a\tb  c'>x</p>");
    let p = element(&document, "p");
    let class = || {
        path(vec![step(
            Axis::Attribute,
            NodeTest::Name(QName {
                prefix: None,
                local: "class".into(),
            }),
            Vec::new(),
        )])
    };
    let literal = |s: &str| Expr::Literal(s.as_bytes().to_vec());
    let css_class = |needle| Expr::Call(Function::CssClass, vec![class(), literal(needle)]);
    assert!(holds(&document, p, css_class("b")));
    assert!(holds(&document, p, css_class("c")));
    assert!(holds(&document, p, css_class("")));
    assert!(!holds(&document, p, css_class("a b")));
    let normalized = Expr::Call(Function::NormalizeSpace, vec![class()]);
    assert!(holds(
        &document,
        p,
        Expr::compare(Comparison::Equal, normalized, literal("a b c"))
    ));
    let concat = Expr::Call(
        Function::Concat,
        vec![literal("x"), number(1.0), literal("y")],
    );
    assert!(holds(
        &document,
        p,
        Expr::compare(Comparison::Equal, concat, literal("x1y"))
    ));
    assert!(holds(
        &document,
        p,
        Expr::Call(Function::EndsWith, vec![class(), literal("c")])
    ));
    let context = Expr::Path(Path::context_node());
    assert!(holds(
        &document,
        p,
        Expr::Call(Function::Contains, vec![context, literal("x")])
    ));
}

#[test]
fn numbers_convert_as_libxml2_converts_them() {
    assert_eq!(string_to_number(b" 12 "), 12.0);
    assert_eq!(string_to_number(b"-1.5"), -1.5);
    assert_eq!(string_to_number(b".5"), 0.5);
    assert_eq!(string_to_number(b"1e2"), 100.0);
    for not_a_number in [&b""[..], b"1 2", b"+1", b"x", b"1.2.3", b"-"] {
        assert!(string_to_number(not_a_number).is_nan(), "{not_a_number:?}");
    }
    // what Nokogiri 1.19.4's libxml2 returns for `string(…)` of each
    for (number, string) in [
        (3.0, "3"),
        (-0.5, "-0.5"),
        (-0.0, "0"),
        (f64::NAN, "NaN"),
        (f64::NEG_INFINITY, "-Infinity"),
        (1e9, "1000000000"),
        (2147483646.0, "2147483646"),
        (2147483647.0, "2.147483647e+09"),
        (-2147483648.0, "-2.147483648e+09"),
        (1e10, "1e+10"),
        (1000000000.5, "1.0000000005e+09"),
        (999999999.5, "999999999.5"),
        (123456789012345678.0, "1.23456789012346e+17"),
        (0.30000000000000004, "0.3"),
        (1.0 / 3.0, "0.333333333333333"),
        (100.0 / 3.0, "33.3333333333333"),
        (123456789.12345679, "123456789.123457"),
        (0.00001, "0.00001"),
        (0.0000123, "0.0000123"),
        (0.000009999, "9.999e-06"),
        (0.000001, "1e-06"),
        (5e-7, "5e-07"),
    ] {
        assert_eq!(number_to_string(number), string, "{number}");
    }
}

#[test]
fn descendant_steps_merge_unless_positions_matter() {
    let mut expr = path(vec![
        step(Axis::DescendantOrSelf, NodeTest::Node, Vec::new()),
        step(Axis::Child, NodeTest::Any, Vec::new()),
        step(Axis::DescendantOrSelf, NodeTest::Node, Vec::new()),
        step(Axis::Child, NodeTest::Any, vec![number(1.0)]),
    ]);
    expr.merge_descendant_steps();
    let Expr::Path(path) = expr else {
        unreachable!()
    };
    let axes: Vec<Axis> = path.steps.iter().map(|step| step.axis).collect();
    assert_eq!(
        axes,
        [Axis::Descendant, Axis::DescendantOrSelf, Axis::Child]
    );
}
