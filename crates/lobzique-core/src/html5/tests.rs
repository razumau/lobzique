use super::*;
use crate::dom::{Document, DocumentKind, NamespaceRef, NodeId, NodeKind};

fn parse(input: &str) -> Document {
    let (document, _) = parse_document(input.as_bytes(), &Options::default()).unwrap();
    document.check_invariants().unwrap();
    document
}

fn html(document: &Document, id: NodeId) -> String {
    String::from_utf8(serialize(document, id, false)).unwrap()
}

fn fragment(input: &str, context: &Context) -> (Document, NodeId) {
    let mut document = Document::new(DocumentKind::Html);
    let fragment = document.create_fragment();
    parse_fragment(
        &mut document,
        fragment,
        input.as_bytes(),
        context,
        &Options::default(),
    )
    .unwrap();
    document.check_invariants().unwrap();
    (document, fragment)
}

fn find(document: &Document, local: &str) -> NodeId {
    document
        .descendants(NodeId::DOCUMENT)
        .find(
            |&id| matches!(document.node_kind(id), NodeKind::Element(e) if &*e.name.local == local),
        )
        .unwrap()
}

#[test]
fn documents_round_trip() {
    let input = "<!DOCTYPE html><html><head><title>t</title></head>\
                 <body><p class=\"a\">x &amp; y<br>z</p></body></html>";
    assert_eq!(html(&parse(input), NodeId::DOCUMENT), input);
}

#[test]
fn the_doctype_is_the_first_child_wherever_it_was() {
    let document = parse("<!--c--><!DOCTYPE html>");
    assert_eq!(
        html(&document, NodeId::DOCUMENT),
        "<!DOCTYPE html><!--c--><html><head></head><body></body></html>"
    );
}

#[test]
fn adjacent_text_is_one_node() {
    let document = parse("<table>a<tr>b</tr>c</table>");
    let body = find(&document, "body");
    let first = document.first_child(body).unwrap();
    assert!(matches!(document.node_kind(first), NodeKind::Text(t) if t == b"abc"));
}

#[test]
fn template_contents_are_its_children() {
    let document = parse("<template><p>x</p></template>");
    let template = find(&document, "template");
    assert_eq!(html(&document, template), "<template><p>x</p></template>");
}

#[test]
fn foreign_elements_have_gumbos_prefixes() {
    let document = parse("<svg xlink:href=x xmlns:xlink=y><foreignObject/></svg><math></math>");
    let svg = find(&document, "svg");
    assert_eq!(
        document.namespace(svg),
        Some(NamespaceRef {
            prefix: Some("svg"),
            href: "http://www.w3.org/2000/svg"
        })
    );
    let prefixes: Vec<_> = document
        .attributes(svg)
        .iter()
        .map(|&a| document.namespace(a).unwrap().prefix)
        .collect();
    assert_eq!(prefixes, [Some("xlink"), Some("xmlns")]);
    assert_eq!(
        document.namespace(find(&document, "math")).unwrap().prefix,
        Some("math")
    );
    assert_eq!(document.namespace(find(&document, "body")), None);
    assert_eq!(
        html(&document, svg),
        "<svg xlink:href=\"x\" xmlns:xlink=\"y\"><foreignObject></foreignObject></svg>"
    );
}

#[test]
fn escaping_follows_nokogiri() {
    let document = parse("<p title='\"&<>\u{a0}'>&lt;&gt;&amp;\"\u{a0}</p><script>a<b</script>");
    let body = find(&document, "body");
    assert_eq!(
        html(&document, body),
        "<body><p title=\"&quot;&amp;<>&nbsp;\">&lt;&gt;&amp;\"&nbsp;</p><script>a<b</script></body>"
    );
}

#[test]
fn preserve_newline_doubles_a_leading_newline() {
    let document = parse("<pre>\n\nx</pre>");
    let pre = find(&document, "pre");
    assert!(prepend_newline(&document, pre));
    assert_eq!(html(&document, pre), "<pre>\nx</pre>");
    assert_eq!(serialize(&document, pre, true), b"<pre>\n\nx</pre>");
}

#[test]
fn noscript_is_markup_unless_asked() {
    let document = parse("<body><noscript><p>x</p></noscript>");
    assert!(matches!(
        document.node_kind(document.first_child(find(&document, "noscript")).unwrap()),
        NodeKind::Element(_)
    ));
}

#[test]
fn fragments_parse_in_their_context() {
    let (document, root) = fragment("<td>x</td>", &Context::new(ContextNamespace::Html, "TR"));
    assert_eq!(html(&document, root), "<td>x</td>");
    let (document, root) = fragment("<td>x</td>", &Context::new(ContextNamespace::Html, "body"));
    assert_eq!(html(&document, root), "x");
    let (document, root) = fragment("<p>x", &Context::new(ContextNamespace::Svg, "svg"));
    assert_eq!(
        document.namespace(document.first_child(root).unwrap()),
        None
    );
    assert_eq!(html(&document, root), "<p>x</p>");
}

#[test]
fn parsing_in_context_leaves_each_node_unlinked() {
    let mut document = Document::new(DocumentKind::Html);
    let context = Context::new(ContextNamespace::Html, "div");
    let (nodes, _) = parse_in_context(
        &mut document,
        b"<p>a</p>b<svg/>",
        &context,
        &Options::default(),
    )
    .unwrap();
    document.check_invariants().unwrap();
    let parts: Vec<_> = nodes.iter().map(|&id| html(&document, id)).collect();
    assert_eq!(parts, ["<p>a</p>", "b", "<svg></svg>"]);
    for &id in &nodes {
        assert_eq!(document.parent(id), None);
        assert_eq!(document.previous_sibling(id), None);
        assert_eq!(document.next_sibling(id), None);
    }
    assert!(document
        .namespace_definitions(nodes[2])
        .iter()
        .any(|ns| ns.prefix.as_deref() == Some("svg")));
}

#[test]
fn parsing_in_context_reuses_its_scaffolding() {
    let mut document = Document::new(DocumentKind::Html);
    let context = Context::new(ContextNamespace::Html, "div").with_form_ancestor(true);
    let parse = |document: &mut Document, input: &[u8]| {
        parse_in_context(document, input, &context, &Options::default())
            .unwrap()
            .0
    };
    let first = parse(&mut document, b"<html lang=en><p>a</p>");
    let before = document.arena_ids().len();
    let second = parse(&mut document, b"<p>b</p>");
    // the p and its text
    assert_eq!(document.arena_ids().len(), before + 2);
    assert_eq!(document.node_ids().count() + 4, document.arena_ids().len());
    document.check_invariants().unwrap();
    assert_eq!(html(&document, first[0]), "<p>a</p>");
    assert_eq!(html(&document, second[0]), "<p>b</p>");
}

/// Parses `input` in the context `Context::for_node` finds for the first
/// `local` element of `page`, or for the document if `local` is empty.
fn in_context_of(page: &str, local: &str, parsed: bool, input: &str) -> (Document, Vec<NodeId>) {
    let mut document = parse(page);
    let node = if local.is_empty() {
        NodeId::DOCUMENT
    } else {
        find(&document, local)
    };
    let context = Context::for_node(&document, node, parsed);
    let (nodes, _) = parse_in_context(
        &mut document,
        input.as_bytes(),
        &context,
        &Options::default(),
    )
    .unwrap();
    document.check_invariants().unwrap();
    (document, nodes)
}

#[test]
fn a_nodes_context_is_the_nearest_element() {
    let (document, nodes) = in_context_of("<table><tr><td>", "tr", true, "<td>x</td>");
    assert_eq!(html(&document, nodes[0]), "<td>x</td>");
    let (_, nodes) = in_context_of("<form><div>", "div", true, "<form></form>");
    assert!(nodes.is_empty());
    let (document, nodes) = in_context_of("<svg><g>", "g", true, "<p>x");
    assert_eq!(document.namespace(nodes[0]), None);
    let page = "<math><annotation-xml encoding=text/html><mi>";
    let (document, nodes) = in_context_of(page, "annotation-xml", true, "<x>");
    assert_eq!(document.namespace(nodes[0]), None);
    let page = "<math><annotation-xml><mi>";
    let (document, nodes) = in_context_of(page, "annotation-xml", true, "<x>");
    assert_eq!(document.namespace(nodes[0]).unwrap().prefix, Some("math"));
}

#[test]
fn a_nodes_context_takes_the_quirks_mode_of_a_parsed_document() {
    let (document, nodes) = in_context_of("<p>", "", true, "<p><table>");
    assert_eq!(html(&document, nodes[0]), "<p><table></table></p>");
    let (document, nodes) = in_context_of("<p>", "", false, "<p><table>");
    assert_eq!(html(&document, nodes[0]), "<p></p>");
    let (document, nodes) = in_context_of("<!DOCTYPE html>", "body", true, "<p><table>");
    assert_eq!(html(&document, nodes[0]), "<p></p>");
}

#[test]
fn a_form_ancestor_drops_nested_forms() {
    let body = Context::new(ContextNamespace::Html, "body");
    let (document, root) = fragment("<form></form>", &body.clone().with_form_ancestor(true));
    assert_eq!(html(&document, root), "");
    let (document, root) = fragment("<form></form>", &body);
    assert_eq!(html(&document, root), "<form></form>");
}

#[test]
fn quirks_mode_comes_from_the_doctype() {
    assert_eq!(
        quirks_mode_for_doctype(Some(b"html"), None, None),
        QuirksMode::NoQuirks
    );
    assert_eq!(
        quirks_mode_for_doctype(None, None, None),
        QuirksMode::Quirks
    );
    let public = b"-//W3C//DTD HTML 4.01 Transitional//EN";
    assert_eq!(
        quirks_mode_for_doctype(Some(b"html"), Some(public), Some(b"x")),
        QuirksMode::LimitedQuirks
    );
    let (_, parsed) = parse_document(b"<p>", &Options::default()).unwrap();
    assert_eq!(parsed.quirks_mode, QuirksMode::Quirks);
}

#[test]
fn fragments_report_their_contexts_quirks_mode() {
    let mut document = Document::new(DocumentKind::Html);
    let root = document.create_fragment();
    let context = Context::new(ContextNamespace::Html, "body").with_quirks_mode(QuirksMode::Quirks);
    let parsed = parse_fragment(
        &mut document,
        root,
        b"<p><table>",
        &context,
        &Options::default(),
    )
    .unwrap();
    assert_eq!(parsed.quirks_mode, QuirksMode::Quirks);
    assert_eq!(html(&document, root), "<p><table></table></p>");
}

#[test]
fn cdata_sections_type_their_text_as_gumbo_does() {
    let svg = |input: &str| {
        let document = parse(input);
        html(&document, find(&document, "svg"))
    };
    assert_eq!(
        svg("<svg><![CDATA[a<b]]></svg>"),
        "<svg><![CDATA[a<b]]></svg>"
    );
    // A run of text takes the type of its last non-whitespace part
    assert_eq!(
        svg("<svg>a<![CDATA[b]]> </svg>"),
        "<svg><![CDATA[ab ]]></svg>"
    );
    assert_eq!(svg("<svg><![CDATA[a]]>b</svg>"), "<svg>ab</svg>");
    // A NUL in a section stays in it; one after it follows a parse error
    assert_eq!(
        svg("<svg><![CDATA[a\0b]]>c</svg>"),
        "<svg>a\u{fffd}bc</svg>"
    );
    assert_eq!(
        svg("<svg><![CDATA[a\0b]]></svg>"),
        "<svg><![CDATA[a\u{fffd}b]]></svg>"
    );
    assert_eq!(
        svg("<svg><![CDATA[a]]>\0</svg>"),
        "<svg><![CDATA[a\u{fffd}]]></svg>"
    );
    // Not a section: a bogus comment
    assert_eq!(
        svg("<svg><![CDATB[a]]></svg>"),
        "<svg><!--[CDATB[a]]--></svg>"
    );
}

#[test]
fn depth_counts_open_elements() {
    let options = |depth| Options {
        max_tree_depth: Some(depth),
        ..Options::default()
    };
    // html, body and two divs; the br is never open
    let input = b"<div><div><br>";
    assert!(parse_document(input, &options(4)).is_ok());
    assert_eq!(
        parse_document(input, &options(3)).err(),
        Some(LimitError::TreeTooDeep)
    );
    let mut document = Document::new(DocumentKind::Html);
    let root = document.create_fragment();
    let body = Context::new(ContextNamespace::Html, "body");
    assert!(parse_fragment(&mut document, root, b"<div><div>", &body, &options(2)).is_ok());
    assert_eq!(
        parse_fragment(&mut document, root, b"<div><div>", &body, &options(1)).err(),
        Some(LimitError::TreeTooDeep)
    );
}

#[test]
fn deep_trees_cost_no_stack() {
    // Deeper input is slow rather than dangerous: html5ever's scope checks
    // walk the stack of open elements, so time grows with depth squared.
    // Miri would take hours at full depth
    let depth = if cfg!(miri) { 100 } else { 5_000 };
    let parse_and_serialize = move || {
        let input = "<div>".repeat(depth);
        let (document, _) = parse_document(input.as_bytes(), &Options::default()).unwrap();
        serialize(&document, NodeId::DOCUMENT, false).len()
    };
    let small_stack = std::thread::Builder::new().stack_size(256 * 1024);
    let length = small_stack
        .spawn(parse_and_serialize)
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(length, 11 * depth + 39);
}

#[test]
fn attributes_are_counted_without_duplicates() {
    let options = Options {
        max_attributes: Some(2),
        ..Options::default()
    };
    assert!(parse_document(b"<p a b a b>", &options).is_ok());
    assert_eq!(
        parse_document(b"</p a b c>", &options).err(),
        Some(LimitError::TooManyAttributes)
    );
}

#[test]
fn errors_stop_at_the_limit() {
    let options = |max_errors| Options {
        max_errors,
        ..Options::default()
    };
    let input = b"<p>\n</b></i></u>";
    let (_, parsed) = parse_document(input, &options(Some(0))).unwrap();
    assert!(parsed.errors.is_empty());
    let (_, parsed) = parse_document(input, &options(Some(2))).unwrap();
    assert_eq!(parsed.errors.len(), 2);
    let (_, parsed) = parse_document(input, &options(None)).unwrap();
    assert!(parsed.errors.len() > 2);
    assert_eq!(parsed.errors.last().unwrap().line, 2);
}

#[test]
fn invalid_utf8_is_replaced() {
    let (document, _) = parse_document(b"<p>\xff</p>", &Options::default()).unwrap();
    assert_eq!(
        document.content(find(&document, "p")).unwrap(),
        "\u{fffd}".as_bytes()
    );
}

fn definitions(document: &Document, id: NodeId) -> Vec<&str> {
    document
        .namespace_definitions(id)
        .iter()
        .map(|definition| NamespaceRef::from(definition).prefix.unwrap())
        .collect()
}

#[test]
fn foreign_namespaces_are_defined_on_the_html_element() {
    let document = parse(
        "<p><svg xmlns=\"http://www.w3.org/2000/svg\"><a xlink:href=x xml:lang=en/></svg><math></math>",
    );
    let html = find(&document, "html");
    assert_eq!(
        definitions(&document, html),
        ["svg", "xmlns", "xlink", "math"]
    );
    assert!(definitions(&document, find(&document, "svg")).is_empty());
    assert!(definitions(&parse("<p>"), NodeId::DOCUMENT).is_empty());
}

#[test]
fn fragment_namespaces_are_defined_on_each_top_level_element() {
    let context = Context::new(ContextNamespace::Html, "body");
    let (document, fragment) = fragment("<div><svg/></div>text<math/><p></p>", &context);
    let elements: Vec<_> = document.element_children(fragment).collect();
    let defined: Vec<_> = elements
        .iter()
        .map(|&element| definitions(&document, element))
        .collect();
    assert_eq!(defined, [vec!["svg"], vec!["math"], vec![]]);
}

#[test]
fn fragments_in_xml_documents_have_no_namespace() {
    let mut document = Document::new(DocumentKind::Xml);
    let fragment = document.create_fragment();
    let context = Context::new(ContextNamespace::Html, "body");
    parse_fragment(
        &mut document,
        fragment,
        b"<p>a<svg></svg></p>",
        &context,
        &Options::default(),
    )
    .unwrap();
    document.check_invariants().unwrap();
    let p = document.first_child(fragment).unwrap();
    let svg = document.last_child(p).unwrap();
    let namespace = |document: &Document, id| match document.node_kind(id) {
        NodeKind::Element(element) => element.name.ns.clone(),
        _ => unreachable!(),
    };
    assert_eq!(
        (namespace(&document, p), namespace(&document, svg)),
        (ns!(), ns!(svg))
    );
    assert_eq!(html(&document, fragment), "<p>a<svg></svg></p>");

    // The second parse builds in older nodes, and renames only what it made
    let html_p = document.create_element(QualName::new(
        None,
        ns!(html),
        crate::dom::LocalName::from("p"),
    ));
    parse_fragment(
        &mut document,
        fragment,
        b"<i></i>",
        &context,
        &Options::default(),
    )
    .unwrap();
    let i = document.last_child(fragment).unwrap();
    assert_eq!(namespace(&document, html_p), ns!(html));
    assert_eq!(namespace(&document, i), ns!());
}
