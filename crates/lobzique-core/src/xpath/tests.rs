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
    let Ok(Outcome::Value(Value::NodeSet(nodes))) = evaluate(
        document,
        &query,
        context,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    ) else {
        panic!("expected nodes");
    };
    describe(document, nodes)
}

fn describe(document: &Document, nodes: Vec<NodeId>) -> Vec<String> {
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
        path(vec![step(Axis::Child, NodeTest::Any, vec![number(3.0)])]),
        path(vec![step(Axis::Child, NodeTest::Any, vec![number(1.0)])]),
        path(vec![step(Axis::Child, NodeTest::Any, vec![number(3.0)])]),
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
    // each as Nokogiri 1.19.4 reads it: digits accumulate as a double
    assert_eq!(string_to_number(b"1e"), 1.0);
    assert_eq!(string_to_number(b"1e+"), 1.0);
    assert_eq!(string_to_number(b" -.5 "), -0.5);
    assert_eq!(string_to_number(b"1."), 1.0);
    assert_eq!(string_to_number(b"1e-400"), 0.0);
    assert_eq!(
        string_to_number(b"12345678901234567890"),
        1.234567890123457e19
    );
    assert_eq!(
        string_to_number(b"0.1234567890123456789012345"),
        0.12345678901234569
    );
    for not_a_number in [
        &b""[..],
        b"1 2",
        b"+1",
        b"x",
        b"1.2.3",
        b"-",
        b".",
        b"0e999999",
    ] {
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
fn only_the_namespace_axis_is_left_out() {
    assert_eq!(
        parse_xpath(b"//p/namespace::*").map(|_| ()),
        Err(ParseError::Unsupported("the namespace axis".into()))
    );
    // libxml2 rejects it anyway
    assert_eq!(
        parse_xpath(b"string(//p").map(|_| ()),
        Err(ParseError::Syntax(Error::InvalidExpression))
    );
}

/// What an XPath string evaluates to, as Nokogiri would return it.
#[derive(Debug, PartialEq)]
enum Found {
    Nodes(Vec<String>),
    Boolean(bool),
    Number(f64),
    String(String),
    Calls(Vec<String>),
    Error(Error),
}

fn value(document: &Document, context: NodeId, xpath: &str) -> Found {
    evaluate_with(document, context, xpath, &HashMap::new())
}

fn evaluate_with(
    document: &Document,
    context: NodeId,
    xpath: &str,
    variables: &HashMap<String, Vec<u8>>,
) -> Found {
    let query = parse_xpath(xpath.as_bytes()).unwrap_or_else(|e| panic!("{xpath}: {e:?}"));
    match evaluate(
        document,
        &query,
        context,
        &HashMap::new(),
        variables,
        &HashMap::new(),
    ) {
        Ok(Outcome::Value(Value::NodeSet(nodes))) => Found::Nodes(describe(document, nodes)),
        Ok(Outcome::Value(Value::Boolean(b))) => Found::Boolean(b),
        Ok(Outcome::Value(Value::Number(n))) => Found::Number(n),
        Ok(Outcome::Value(Value::String(s))) => {
            Found::String(String::from_utf8(s.into_owned()).unwrap())
        }
        Ok(Outcome::Calls(calls)) => Found::Calls(
            calls
                .iter()
                .map(|call| query.function(call.site).local.clone())
                .collect(),
        ),
        Err(error) => Found::Error(error),
    }
}

fn nodes(ids: &[&str]) -> Found {
    Found::Nodes(ids.iter().map(|id| id.to_string()).collect())
}

fn string(s: &str) -> Found {
    Found::String(s.into())
}

/// Each as Nokogiri 1.19.4 evaluates it.
#[test]
fn values_are_libxml2s() {
    use Found::{Boolean, Error as E, Number};
    let document = parse(
        "<html lang=en><body><p id=a class='x y'>One <b>two</b></p><p id=b>3</p>\
         <svg xml:lang=en-US><g id=g><a name=m></a></g></svg><math><mi xlink:href=u>x</mi></math>",
    );
    let a = element(&document, "a");
    for (xpath, found) in [
        ("count(//p)", Number(2.0)),
        ("sum(//p)", Number(f64::NAN)),
        ("sum(//p[2])", Number(3.0)),
        ("sum(//nosuch)", Number(0.0)),
        ("string()", string("One two")),
        ("string(//p)", string("One two")),
        ("string(1e30)", string("1e+30")),
        ("string(-0)", string("0")),
        ("string(0.1 + 0.2)", string("0.3")),
        ("string(1 div 0)", string("Infinity")),
        ("number('12')", Number(12.0)),
        ("number()", Number(f64::NAN)),
        ("boolean('')", Boolean(false)),
        ("boolean(//p)", Boolean(true)),
        ("local-name()", string("p")),
        ("name()", string("p")),
        ("name(//*[local-name() = 'svg'])", string("svg:svg")),
        (
            "namespace-uri(//*[local-name() = 'svg'])",
            string("http://www.w3.org/2000/svg"),
        ),
        ("name(//*[local-name() = 'mi']/@*)", string("xlink:href")),
        ("local-name(//*[local-name() = 'mi']/@*)", string("href")),
        ("name(/)", string("")),
        ("name(//text())", string("")),
        ("substring('12345', 2, 3)", string("234")),
        ("substring('12345', 1.5, 2.6)", string("234")),
        ("substring('12345', 0, 3)", string("12")),
        ("substring('12345', 0 div 0, 3)", string("")),
        ("substring('12345', 1, 0 div 0)", string("")),
        ("substring('12345', -42, 1 div 0)", string("12345")),
        ("substring('12345', -1 div 0, 1 div 0)", string("")),
        ("substring('ébc', 2)", string("bc")),
        ("substring-before('a/b/c', '/')", string("a")),
        ("substring-after('a/b/c', '/')", string("b/c")),
        ("substring-after('abc', '')", string("abc")),
        ("translate('bar', 'abc', 'ABC')", string("BAr")),
        ("translate('--aaa--', 'abc-', 'ABC')", string("AAA")),
        ("translate('éa', 'é', 'E')", string("Ea")),
        ("string-length('héllo')", Number(5.0)),
        ("string-length()", Number(7.0)),
        ("normalize-space('  a  b ')", string("a b")),
        ("concat('a', 1, true())", string("a1true")),
        ("round(2.5)", Number(3.0)),
        ("round(-2.5)", Number(-2.0)),
        ("floor(-1.5)", Number(-2.0)),
        ("1 + 2 * 3 - 4 div 8", Number(6.5)),
        ("-5 mod 2", Number(-1.0)),
        ("5.5 mod 2", Number(1.5)),
        ("1 div -0", Number(f64::NEG_INFINITY)),
        ("-(1)", Number(-1.0)),
        ("--'3'", Number(3.0)),
        ("- - 'x'", Number(f64::NAN)),
        ("-//p", Number(-f64::NAN)),
        ("2 div2", Number(1.0)),
        ("1 < 2 < 3", Boolean(true)),
        ("3 > 2 > 1", Boolean(false)),
        ("1 = 1 != 0", Boolean(true)),
        ("'a' < 'b'", Boolean(false)),
        ("0 div 0 = 0 div 0", Boolean(false)),
        ("true() > false()", Boolean(true)),
        ("//p = 3", Boolean(true)),
        ("//nosuch = false()", Boolean(true)),
        ("//nosuch != //p", Boolean(false)),
        ("false() and foo()", Boolean(false)),
        ("true() or foo()", Boolean(true)),
        ("'x'/.", string("x")),
        ("1/.", Number(1.0)),
        ("(//p)[2]", nodes(&["b"])),
        ("(//p)[last()]/@id", nodes(&["b"])),
        ("(//b/ancestor::*)[1]", nodes(&["?"])),
        ("//b/ancestor::*[1]", nodes(&["a"])),
        ("//p | //b", nodes(&["a", "?", "b"])),
        ("//p[position() > 1]", nodes(&["b"])),
        ("//p[last() - 1]", nodes(&["a"])),
        ("//p[-(-2)]", nodes(&["b"])),
        ("//p['x']", nodes(&["a", "b"])),
        ("//p['']", nodes(&[])),
        ("//nosuch[foo()]", nodes(&[])),
        ("id('a b')", nodes(&["a", "b"])),
        ("id(//p/@id)", nodes(&["a", "b"])),
        ("id('m')", nodes(&["?"])),
        ("//*[lang('en')]", nodes(&["?", "g", "?"])),
        ("//*[lang('EN-us')]", nodes(&["?", "g", "?"])),
        ("//*[lang('en-')]", nodes(&[])),
        ("//p[lang('en')]", nodes(&[])),
        (
            "//p[nokogiri-builtin:local-name-is('p')]",
            nodes(&["a", "b"]),
        ),
        ("nokogiri-builtin:css-class('a b', 'b')", Boolean(true)),
        ("count(//p/self::processing-instruction())", Number(0.0)),
        ("count('x')", E(Error::InvalidType)),
        ("sum('1')", E(Error::InvalidType)),
        ("name(1)", E(Error::InvalidType)),
        ("'x'[1]", E(Error::InvalidType)),
        ("'x'//.", E(Error::InvalidType)),
        ("1 | //p", E(Error::InvalidType)),
        ("count()", E(Error::InvalidArity)),
        ("true(1)", E(Error::InvalidArity)),
        ("concat('a')", E(Error::InvalidArity)),
        ("substring('a')", E(Error::InvalidArity)),
        ("nokogiri-builtin:css-class('a')", E(Error::InvalidArity)),
        ("$x", E(Error::UndefinedVariable)),
        ("$nokogiri:x", E(Error::UndefinedVariable)),
        ("$bad:x", E(Error::UndefinedNamespacePrefix)),
        ("bar:foo()", E(Error::UndefinedNamespacePrefix)),
        ("foo()", Found::Calls(vec!["foo".into()])),
        // the argument's error comes first
        ("count(foo(), 'x')", Found::Calls(vec!["foo".into()])),
    ] {
        let actual = value(&document, a, xpath);
        let same = match (&actual, &found) {
            (Number(x), Number(y)) => x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan()),
            _ => actual == found,
        };
        assert!(same, "{xpath}: {actual:?}, expected {found:?}");
    }
    // negative zero survives
    assert_eq!(value(&document, a, "round(-0.4)"), Number(-0.0));
    assert!(
        matches!(value(&document, a, "ceiling(-0.5)"), Number(n) if n == 0.0 && n.is_sign_negative())
    );
}

#[test]
fn processing_instructions() {
    let mut document = parse("<p id=p>");
    let p = element(&document, "p");
    let pi = document.create_processing_instruction(b"tgt".to_vec(), b"data".to_vec());
    document.append_child(p, pi).unwrap();
    let found = |xpath| value(&document, NodeId::DOCUMENT, xpath);
    assert_eq!(found("//processing-instruction()"), nodes(&["data"]));
    assert_eq!(found("//processing-instruction('tgt')"), nodes(&["data"]));
    assert_eq!(found("//processing-instruction('x')"), nodes(&[]));
    assert_eq!(found("name(//processing-instruction())"), string("tgt"));
    assert_eq!(
        found("local-name(//processing-instruction())"),
        string("tgt")
    );
    assert_eq!(found("count(//p/node())"), Found::Number(1.0));
}

#[test]
fn variables_are_strings() {
    let document = parse("<p id=a>1</p><p id=b>2</p>");
    let variables = HashMap::from([("x".to_string(), b"2".to_vec())]);
    let found = |xpath| evaluate_with(&document, NodeId::DOCUMENT, xpath, &variables);
    assert_eq!(found("$x"), string("2"));
    assert_eq!(found("$x + 1"), Found::Number(3.0));
    assert_eq!(found("//p[. = $x]"), nodes(&["b"]));
    // a string, so not a position
    assert_eq!(found("//p[$x]"), nodes(&["a", "b"]));
    assert_eq!(found("$x[1]"), Found::Error(Error::InvalidType));
    assert_eq!(found("$y"), Found::Error(Error::UndefinedVariable));
}

#[test]
fn ids_are_the_first_of_each() {
    let document = parse("<a name=n id=z></a><p id=a>1</p><p id=a>2</p><i id='x y'></i>");
    let found = |xpath| value(&document, NodeId::DOCUMENT, xpath);
    assert_eq!(found("id('n')"), nodes(&["z"]));
    assert_eq!(found("count(id('a'))"), Found::Number(1.0));
    assert_eq!(found("string(id('a'))"), string("1"));
    // tokens split at blanks
    assert_eq!(found("id('x y')"), nodes(&[]));
}

#[test]
fn custom_calls_from_xpath_strings() {
    let document = parse("<p id=a>1</p><p id=b>2</p>");
    let query = parse_xpath(b"//p[nokogiri:keep(., 'x')]/@id | //p[mine()]").unwrap();
    assert_eq!(
        query.function(0),
        &FunctionName {
            prefix: Some("nokogiri".into()),
            local: "keep".into()
        }
    );
    assert_eq!(query.function(1).prefix, None);
    let none = HashMap::new();
    let Ok(Outcome::Calls(calls)) = evaluate(
        &document,
        &query,
        NodeId::DOCUMENT,
        &none,
        &none_variables(),
        &HashMap::new(),
    ) else {
        panic!("calls expected");
    };
    assert_eq!(calls.len(), 4);
    assert_eq!(calls[0].arguments[1], Value::String(b"x"[..].into()));
    let answers = calls
        .iter()
        .map(|call| ((call.site, call.node), Ok(Value::Boolean(call.site == 0))))
        .collect();
    let Ok(Outcome::Value(Value::NodeSet(found))) = evaluate(
        &document,
        &query,
        NodeId::DOCUMENT,
        &none,
        &none_variables(),
        &answers,
    ) else {
        panic!("nodes expected");
    };
    assert_eq!(describe(&document, found), ["a", "b"]);
}

/// The empty node set standing in for an answer can reach calls libxml2
/// never makes; their errors count only once every call is answered.
#[test]
fn errors_answer_calls_only_when_reached() {
    let document = parse("<p id=a>");
    let query = parse_xpath(b"nokogiri:yes() or nokogiri:missing()").unwrap();
    let none = HashMap::new();
    let mut answers = HashMap::new();
    let run = |answers: &HashMap<_, _>| {
        evaluate(
            &document,
            &query,
            NodeId::DOCUMENT,
            &none,
            &none_variables(),
            answers,
        )
    };
    let Ok(Outcome::Calls(calls)) = run(&answers) else {
        panic!("calls expected");
    };
    assert_eq!(calls.len(), 2);
    answers.insert((calls[0].site, calls[0].node), Ok(Value::Boolean(true)));
    answers.insert(
        (calls[1].site, calls[1].node),
        Err(Error::UnregisteredFunction),
    );
    assert_eq!(run(&answers), Ok(Outcome::Value(Value::Boolean(true))));
    answers.insert((calls[0].site, calls[0].node), Ok(Value::Boolean(false)));
    assert_eq!(run(&answers), Err(Error::UnregisteredFunction));
}

fn none_variables() -> HashMap<String, Vec<u8>> {
    HashMap::new()
}

/// Operators of one precedence make a list, so a long chain of them costs
/// no stack to evaluate or drop.
#[test]
fn long_operator_chains_stay_flat() {
    let document = parse("<p>");
    let sum = format!("1{}", " + 1".repeat(100_000));
    assert_eq!(
        value(&document, NodeId::DOCUMENT, &sum),
        Found::Number(100_001.0)
    );
    let signs = format!("{}1", "-".repeat(100_001));
    assert_eq!(
        value(&document, NodeId::DOCUMENT, &signs),
        Found::Number(-1.0)
    );
    let comparisons = format!("1{}", " = 1".repeat(100_000));
    assert_eq!(
        value(&document, NodeId::DOCUMENT, &comparisons),
        Found::Boolean(true)
    );
    let union = format!("//p{}", " | //p".repeat(10_000));
    assert_eq!(value(&document, NodeId::DOCUMENT, &union), nodes(&["?"]));
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
        (
            format!("{}]", nested(MAX_NESTING + 1)),
            Error::InvalidExpression,
        ),
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
