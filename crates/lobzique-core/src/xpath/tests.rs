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
    expr.merge_descendant_steps(ast::Merge::UnlessPositional);
    let Expr::Path(path) = expr else {
        unreachable!()
    };
    let axes: Vec<Axis> = path.steps.iter().map(|step| step.axis).collect();
    assert_eq!(
        axes,
        [Axis::Descendant, Axis::DescendantOrSelf, Axis::Child]
    );
}

/// The ids, or contents, of what the XPath string `xpath` finds.
fn find(document: &Document, context: NodeId, xpath: &str) -> Vec<String> {
    let query = parse_xpath(xpath.as_bytes()).unwrap_or_else(|e| panic!("{xpath}: {e:?}"));
    run(document, context, query.expr)
}

fn parse_xpath(xpath: &[u8]) -> Result<Query, ParseError> {
    super::parse(xpath)
}

#[test]
fn xpath_strings_find_what_libxml2_finds() {
    let document = parse(SIBLINGS);
    let d = element(&document, "d");
    let c = element(&document, "c");
    for (xpath, found) in [
        ("//p", &["a", "b", "c"][..]),
        ("/html/body/div/p[2]", &["b"]),
        ("p", &["a", "b", "c"]),
        ("./p[b]", &["a"]),
        ("p[.//i]/following-sibling::*", &[]),
        ("p[@id='b']/preceding-sibling::p", &["a"]),
        ("//p[@id = \"c\"]/i", &["ci"]),
        ("..", &["?"]),
        ("// p [ 3 ] / i", &["ci"]),
        ("child :: p[2.]", &["b"]),
        ("p[--1]", &["a"]),
        ("p[20e-1]", &["b"]),
        ("p[1e]", &["a"]),
        ("p[1.5]", &[]),
        ("descendant::*[@id][1]", &["a"]),
        ("*:p[last]", &[]),
        ("p[3]/descendant-or-self::node()", &["c", "ci"]),
        ("/ /html/body/div/p[1]", &["a"]),
        ("/ //i", &["ci"]),
    ] {
        assert_eq!(find(&document, d, xpath), found, "{xpath}");
    }
    assert_eq!(find(&document, c, "preceding::*[1]"), ["b"]);
    // libxml2 reads an overlong form in a name as what it encodes
    let overlong_p = parse_xpath(b"\xC1\xB0[1]").unwrap();
    assert_eq!(run(&document, d, overlong_p.expr), ["a"]);
    assert_eq!(find(&document, c, "ancestor::*[1]"), ["d"]);
}

#[test]
fn doctypes_pass_no_node_test() {
    let document = parse("<!DOCTYPE html><p id=p>");
    // <html>
    assert_eq!(find(&document, NodeId::DOCUMENT, "/node()"), ["?"]);
    // <head>
    let p = element(&document, "p");
    assert_eq!(find(&document, p, "preceding::node()"), ["?"]);
}

#[test]
fn following_an_attribute_starts_after_its_element() {
    let document = parse("<p id=a class=x><b id=b></b></p><p id=c></p>");
    let a = element(&document, "a");
    assert_eq!(find(&document, a, "@class/following::*"), ["c"]);
    // <head>
    assert_eq!(find(&document, a, "@class/preceding::*"), ["?"]);
    // Nokogiri's `*:name` matches elements only
    assert_eq!(find(&document, a, "@*:id"), [""; 0]);
}

#[test]
fn syntax_errors_carry_libxml2_codes() {
    // each as Nokogiri 1.19.4 reports it
    for (xpath, error) in [
        (&b""[..], Error::InvalidExpression),
        (b"  ", Error::InvalidExpression),
        (b"..faulty_xpath", Error::InvalidExpression),
        (b"//", Error::InvalidExpression),
        (b"//p/", Error::InvalidExpression),
        (b"//p[", Error::InvalidExpression),
        (b"//p[]", Error::InvalidExpression),
        (b"//p]", Error::InvalidExpression),
        (b"//p@id", Error::InvalidExpression),
        (b"//p/.[1]", Error::InvalidExpression),
        (b"child::", Error::InvalidExpression),
        (b"nosuch-axis::p", Error::InvalidExpression),
        (b"//svg: svg", Error::InvalidExpression),
        (b"//* :p", Error::InvalidExpression),
        (b"//p[string(]", Error::InvalidExpression),
        (b"(//p", Error::InvalidExpression),
        (b"//p[@id='a'", Error::InvalidPredicate),
        (b"//p[1 e1]", Error::InvalidPredicate),
        (b"//p[@id ! = 'a']", Error::InvalidPredicate),
        (b"//p[@id='a", Error::UnfinishedLiteral),
        (b"text(", Error::Unclosed),
        (b"//p[processing-instruction(]", Error::StartLiteral),
        (b"//p[$]", Error::VariableReference),
        (b"//p[$ v]", Error::VariableReference),
        (b"//p[$a:]", Error::VariableReference),
        (b"//\xFF", Error::Encoding),
        (b"//\xC3", Error::Encoding),
        (b"//p[@id='\xFF']", Error::InvalidChar),
        (b"//p[@id='\x01']", Error::InvalidChar),
        // checked before the missing quote
        (b"//p['\x01", Error::InvalidChar),
        (b"//p[\"\xFF", Error::InvalidChar),
        (b"//p[\"a\xC3", Error::InvalidChar),
        (b"//p['\x82\x80']", Error::InvalidChar),
        // libxml2 decodes overlong forms, surrogates and code points past
        // U+10FFFF in names, then finds them out of range
        (b"//\xC0\x80", Error::InvalidChar),
        (b"//\xED\xA0\x80", Error::InvalidChar),
        (b"//p\xF4\x90\x80\x80", Error::InvalidChar),
        (b"/\xF4\x90\x80\x80", Error::InvalidChar),
        (b"//\xEF\xBF\xBE", Error::InvalidChar),
    ] {
        let result = parse_xpath(xpath).map(|_| ());
        assert_eq!(result, Err(ParseError::Syntax(error)), "{xpath:?}");
    }
}

#[test]
fn constructs_outside_the_subset_are_named() {
    for (xpath, construct) in [
        ("string(//title)", "function string()"),
        ("//p[nokogiri:thing(.)]", "function nokogiri:thing()"),
        ("//p | //br", "unions"),
        ("(//p)[1]", "parentheses"),
        ("//p[$v]", "variable $v"),
        ("//p[@a != 'x']", "the != operator"),
        ("//p[@a and @b]", "the and operator"),
        ("//p[1 or 2]", "the or operator"),
        ("//p[1 + 1]", "the + operator"),
        ("//p[2 div 1]", "the div operator"),
        ("//p[@a < 1]", "the < operator"),
        ("//p[-@a]", "unary minus"),
        ("//p['x']", "literals as predicates"),
        ("//p[@a = 'x' = 'y']", "chained comparisons"),
        ("//svg:*", "the node test svg:*"),
        ("//processing-instruction()", "processing-instruction()"),
        ("//p/namespace::*", "the namespace axis"),
        ("'x'[1]", "filter expressions"),
        ("1 = 1", "expressions that aren't location paths"),
        ("'x'", "expressions that aren't location paths"),
    ] {
        let result = parse_xpath(xpath.as_bytes()).map(|_| ());
        assert_eq!(
            result,
            Err(ParseError::Unsupported(construct.into())),
            "{xpath}"
        );
    }
    // libxml2 rejects it anyway
    assert_eq!(
        parse_xpath(b"string(//p").map(|_| ()),
        Err(ParseError::Syntax(Error::InvalidExpression))
    );
}

#[test]
fn predicates_nest_32_deep() {
    let nested = |depth| format!("{}p{}", "p[".repeat(depth), "]".repeat(depth));
    assert!(parse_xpath(nested(MAX_NESTING).as_bytes()).is_ok());
    assert_eq!(
        parse_xpath(nested(MAX_NESTING + 1).as_bytes()).map(|_| ()),
        Err(ParseError::Unsupported("nesting deeper than 32".into()))
    );
    // a syntax error past the limit still wins, as in libxml2
    let deep = "p[".repeat(MAX_NESTING + 1);
    for (xpath, error) in [
        (format!("{deep}x"), Error::InvalidPredicate),
        (format!("{deep}x)"), Error::InvalidPredicate),
        (format!("{deep}x(]"), Error::InvalidExpression),
        (format!("{deep}'x"), Error::UnfinishedLiteral),
        (format!("{}]", nested(MAX_NESTING + 1)), Error::InvalidExpression),
    ] {
        assert_eq!(
            parse_xpath(xpath.as_bytes()).map(|_| ()),
            Err(ParseError::Syntax(error)),
            "{xpath}"
        );
    }
    let quoted = format!("{deep}'])'{}", "]".repeat(MAX_NESTING + 1));
    assert_eq!(
        parse_xpath(quoted.as_bytes()).map(|_| ()),
        Err(ParseError::Unsupported("nesting deeper than 32".into()))
    );
}
