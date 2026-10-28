use super::*;
use crate::dom::{Document, NodeId, NodeKind};
use crate::html5::{parse_document, Options as ParseOptions};
use crate::xpath::{evaluate, Outcome, Value};
use std::collections::HashMap;

fn parse(input: &str) -> Document {
    parse_document(input.as_bytes(), &ParseOptions::default())
        .unwrap()
        .0
}

fn options(prefixes: &[Prefix]) -> Options {
    Options {
        html: true,
        builtins: true,
        default_namespace: false,
        prefixes: prefixes.to_vec(),
        first_only: false,
    }
}

/// Each match as its `id`, or its name if it has none, as `Document#css`
/// would find it.
fn select(document: &Document, rule: &str) -> Vec<String> {
    select_from(
        document,
        NodeId::DOCUMENT,
        rule,
        &options(&[Prefix::Global]),
    )
}

fn select_from(document: &Document, context: NodeId, rule: &str, options: &Options) -> Vec<String> {
    let selector = compile(rule, options).unwrap();
    let query = selector.query.unwrap();
    let namespaces = HashMap::new();
    let Outcome::Nodes(nodes) =
        evaluate(document, &query, context, &namespaces, &HashMap::new()).unwrap()
    else {
        panic!("no custom calls expected");
    };
    nodes.into_iter().map(|id| describe(document, id)).collect()
}

fn describe(document: &Document, id: NodeId) -> String {
    match document.node_kind(id) {
        NodeKind::Element(_) => document
            .attribute_value(id, b"id")
            .map(|id| String::from_utf8(id.to_vec()).unwrap())
            .unwrap_or_else(|| {
                String::from_utf8(document.node_name(id).unwrap().to_vec()).unwrap()
            }),
        _ => String::from_utf8(document.content(id).unwrap_or_default()).unwrap(),
    }
}

fn error(rule: &str) -> String {
    compile(rule, &options(&[Prefix::Global])).unwrap_err().0
}

fn invalid(rule: &str) -> bool {
    compile(rule, &options(&[Prefix::Global]))
        .unwrap()
        .query
        .is_err()
}

const LIST: &str = "<div id=d><p id=a class='x y'>1</p><p id=b>2<b id=bold>b</b></p>\
                    <span id=s>s</span><p id=c class=x>3</p></div>";

#[test]
fn simple_selectors() {
    let document = parse(LIST);
    assert_eq!(select(&document, "p"), ["a", "b", "c"]);
    assert_eq!(select(&document, ".x"), ["a", "c"]);
    assert_eq!(select(&document, "#b"), ["b"]);
    assert_eq!(select(&document, "p.x.y"), ["a"]);
    assert_eq!(select(&document, "div > span"), ["s"]);
    assert_eq!(select(&document, "div b"), ["bold"]);
    assert_eq!(select(&document, "p + span"), ["s"]);
    assert_eq!(select(&document, "#a ~ p"), ["b", "c"]);
    assert_eq!(select(&document, "span, #a"), ["a", "s"]);
    assert_eq!(select(&document, "P"), Vec::<String>::new());
}

#[test]
fn positions_count_along_each_step() {
    let document = parse(LIST);
    // position() among the step's nodes: `p` children, or `p.x` ones for *-of-type
    assert_eq!(select(&document, "p:first"), ["a"]);
    assert_eq!(select(&document, "p.x:first"), ["a"]);
    assert_eq!(select(&document, "p:last"), ["c"]);
    assert_eq!(select(&document, ".x:last-of-type"), ["c"]);
    assert_eq!(select(&document, ".x:nth-of-type(2)"), ["c"]);
    assert_eq!(select(&document, "p:nth-child(2)"), ["b"]);
    assert_eq!(select(&document, "p:nth-child(odd)"), ["a"]);
    assert_eq!(select(&document, "p:nth-last-child(1)"), ["c"]);
    // integers as Ruby reads them: bignums, and underscores between digits
    assert_eq!(
        select(&document, "p:nth-child(99999999999999999999n+1)"),
        ["a"]
    );
    assert_eq!(select(&document, "p:nth-child(2n-1_0)"), ["b", "c"]);
    assert_eq!(select(&document, "p:nth-child(2n-1__0)"), ["a"]);
    // `self::` after `+` always counts 1
    assert_eq!(select(&document, "p + p:first"), ["b"]);
    assert_eq!(select(&document, "p[2]"), ["b"]);
}

#[test]
fn nokogiri_quirks() {
    let document = parse(LIST);
    // `:not(p.x)` drops the class
    assert_eq!(select(&document, "p:not(p.x)"), Vec::<String>::new());
    // an empty prefix matches a missing attribute
    assert_eq!(select(&document, "p[class^='']"), ["a", "b", "c"]);
    assert_eq!(select(&document, "p[class='']"), Vec::<String>::new());
    assert_eq!(select(&document, "p[@class]"), ["a", "c"]);
    assert_eq!(select(&document, "div > @class"), Vec::<String>::new());
    assert_eq!(select(&document, "p > @class"), ["x y", "x"]);
    assert_eq!(select(&document, "p > text()"), ["1", "2", "3"]);
    assert_eq!(select(&document, "p:contains('2')"), ["b"]);
    assert_eq!(select(&document, "div:has(b)"), ["d"]);
    assert_eq!(select(&document, "p:has(> b)"), ["b"]);
    assert_eq!(select(&document, "p:has(+ span)"), ["b"]);
    assert_eq!(select(&document, "[class|=x]"), ["c"]);
    assert_eq!(select(&document, "p:empty"), Vec::<String>::new());
    assert_eq!(select(&document, "b:root"), Vec::<String>::new());
}

#[test]
fn dash_match_binds_looser_than_the_conditions_after_it() {
    let document =
        parse("<p id=a lang=en></p><p id=b lang=en-us class=c></p><p id=c lang=en-us></p>");
    // `@lang='en' or starts-with(…) and contains(…class…)`
    assert_eq!(select(&document, "p[lang|=en].c"), ["a", "b"]);
}

#[test]
fn searches_stay_inside_the_context_node() {
    let document = parse("<div id=outer><p id=p1><b id=b1></b></p></div>");
    let p = document
        .descendants(NodeId::DOCUMENT)
        .find(|&id| document.attribute_value(id, b"id") == Some(b"p1"))
        .unwrap();
    let subtree = options(&[Prefix::Subtree]);
    // `.//div//b`: the div would have to be inside the p
    assert!(select_from(&document, p, "div b", &subtree).is_empty());
    assert_eq!(select_from(&document, p, "b", &subtree), ["b1"]);
    let set = options(&[Prefix::Subtree, Prefix::Self_]);
    assert_eq!(select_from(&document, p, "p", &set), ["p1"]);
    assert_eq!(select_from(&document, p, "> b", &set), ["b1"]);
}

#[test]
fn names_without_a_prefix_ignore_namespaces_in_html() {
    let document = parse("<svg><rect id=r /></svg>");
    assert_eq!(select(&document, "rect"), ["r"]);
    let xml = Options {
        html: false,
        ..options(&[Prefix::Global])
    };
    assert!(select_from(&document, NodeId::DOCUMENT, "rect", &xml).is_empty());
}

#[test]
fn syntax_errors() {
    assert_eq!(error(""), "empty CSS selector");
    assert_eq!(error("p:"), "unexpected '$' after ':'");
    assert_eq!(error("p::x"), "unexpected ':' after ':'");
    assert_eq!(error("p:nth-child(2x)"), "parse error on IDENT 'x'");
    assert_eq!(error("p:nth-child(0n-1)"), "divided by 0");
    assert_eq!(error("p:-x"), "Invalid XPath function name '-x'");
    assert!(error("div,").starts_with("unexpected '$'"));
}

#[test]
fn invalid_xpath_waits_for_evaluation() {
    assert!(invalid("a + text()"));
    assert!(invalid("// p"));
    assert!(invalid("#a\\'b"));
    assert!(invalid("p:not(.a:first-of-type)"));
    assert!(invalid("eq(1)"));
    // an unescaped value of one quote opens a literal
    assert!(invalid(r#"p[x=\"]"#));
    assert!(invalid(r"p[x=\']"));
    assert!(!invalid(r#"p[x="\""]"#));
    assert!(!invalid("has(a)"));
    assert!(!invalid("p:checked"));
}

#[test]
fn deep_nesting_is_refused() {
    let nested = |depth| ":not(".repeat(depth) + "p" + &")".repeat(depth);
    assert!(compile(&nested(parser::MAX_NESTING), &options(&[Prefix::Global])).is_ok());
    assert!(compile(
        &nested(parser::MAX_NESTING + 1),
        &options(&[Prefix::Global])
    )
    .is_err());
}

#[test]
fn long_selectors_cost_no_stack() {
    let length = if cfg!(miri) { 200 } else { 20_000 };
    let run = move || {
        let document = parse("<p class=a>x</p>");
        let compound = format!("p{}", ".a".repeat(length));
        let chain = vec!["p"; length].join(" ");
        (
            select(&document, &compound).len(),
            select(&document, &chain).len(),
        )
    };
    let small_stack = std::thread::Builder::new().stack_size(256 * 1024);
    assert_eq!(small_stack.spawn(run).unwrap().join().unwrap(), (1, 0));
}

#[test]
fn nesting_stays_within_a_ruby_thread_stack() {
    // Ruby threads get 1 MB; a debug build takes about four times the
    // stack per level that a release build does
    let run = || {
        let document = parse("<p class=a>x</p>");
        let depth = parser::MAX_NESTING;
        let nested = ":has(".repeat(depth) + "p" + &")".repeat(depth);
        select(&document, &nested).len()
    };
    let stack = std::thread::Builder::new().stack_size(1024 * 1024);
    assert_eq!(stack.spawn(run).unwrap().join().unwrap(), 0);
}

#[test]
fn custom_calls_are_noted_until_answered() {
    let document = parse(LIST);
    let query = compile("p:odd('x')", &options(&[Prefix::Global]))
        .unwrap()
        .query
        .unwrap();
    assert_eq!(query.function_name(0), "odd");
    let namespaces = HashMap::new();
    let mut answers = HashMap::new();
    let Outcome::Calls(calls) =
        evaluate(&document, &query, NodeId::DOCUMENT, &namespaces, &answers).unwrap()
    else {
        panic!("calls expected");
    };
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].arguments[1], Value::String(b"x"[..].into()));
    for (i, call) in calls.iter().enumerate() {
        answers.insert((call.site, call.node), Value::Boolean(i % 2 == 0));
    }
    let Outcome::Nodes(nodes) =
        evaluate(&document, &query, NodeId::DOCUMENT, &namespaces, &answers).unwrap()
    else {
        panic!("nodes expected");
    };
    let ids: Vec<String> = nodes.iter().map(|&id| describe(&document, id)).collect();
    assert_eq!(ids, ["a", "c"]);
}

#[test]
fn prefixes_must_be_bound() {
    let document = parse("<svg><rect id=r /></svg>");
    let query = compile("svg|rect", &options(&[Prefix::Global]))
        .unwrap()
        .query
        .unwrap();
    let unbound = evaluate(
        &document,
        &query,
        NodeId::DOCUMENT,
        &HashMap::new(),
        &HashMap::new(),
    );
    assert_eq!(unbound, Err(crate::xpath::Error::UndefinedNamespacePrefix));
    let bound = HashMap::from([("svg".to_string(), "http://www.w3.org/2000/svg".to_string())]);
    let found = evaluate(&document, &query, NodeId::DOCUMENT, &bound, &HashMap::new());
    assert!(matches!(found, Ok(Outcome::Nodes(nodes)) if nodes.len() == 1));
}

#[test]
fn unescaping() {
    assert_eq!(parser::unescape_identifier(r"a\31 b\.c").unwrap(), "a1b.c");
    assert_eq!(parser::unescape_string("'a\\\nb'").unwrap(), "'ab'");
    // Nokogiri goes on with invalid UTF-8 or a NUL, and fails later
    for selector in [r".\110000", r"#\D800", r"p[x=\0]", r"p[x='\dfff']"] {
        assert!(
            compile(selector, &options(&[Prefix::Global])).is_err(),
            "{selector}"
        );
    }
}

#[test]
fn reduce_handles_the_rules_the_tables_number() {
    assert_eq!(parser::REDUCED_RULES, tables::GRAMMAR);
}
