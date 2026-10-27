use super::*;
use html5ever::{local_name, ns};
use std::cmp::Ordering;

fn html_name(local: &str) -> QualName {
    QualName::new(None, ns!(html), LocalName::from(local))
}

fn html() -> Document {
    Document::new(DocumentKind::Html)
}

fn children(document: &Document, id: NodeId) -> Vec<NodeId> {
    document.children(id).collect()
}

#[test]
fn document_node_type_follows_its_kind() {
    assert_eq!(html().node_type(NodeId::DOCUMENT), NodeType::HtmlDocument);
    let xml = Document::new(DocumentKind::Xml);
    assert_eq!(xml.node_type(NodeId::DOCUMENT), NodeType::Document);
}

#[test]
fn node_names_follow_libxml2() {
    let mut doc = html();
    let element = doc.create_element(html_name("Div"));
    let text = doc.create_text("x".into());
    let comment = doc.create_comment("x".into());
    let cdata = doc.create_cdata("x".into());
    let fragment = doc.create_fragment();
    assert_eq!(doc.node_name(element), Some(&b"Div"[..]));
    assert_eq!(doc.node_name(text), Some(&b"text"[..]));
    assert_eq!(doc.node_name(comment), Some(&b"comment"[..]));
    assert_eq!(doc.node_name(cdata), None);
    assert_eq!(doc.node_name(fragment), None);
    assert_eq!(doc.node_name(NodeId::DOCUMENT), None);
}

#[test]
fn append_and_insert_keep_sibling_order() {
    let mut doc = html();
    let body = doc.create_element(html_name("body"));
    let [a, b, c, d] = ["a", "b", "c", "d"].map(|n| doc.create_element(html_name(n)));
    doc.append_child(NodeId::DOCUMENT, body).unwrap();
    doc.append_child(body, b).unwrap();
    doc.insert_before(b, a).unwrap();
    doc.insert_after(b, d).unwrap();
    doc.insert_after(b, c).unwrap();
    assert_eq!(children(&doc, body), [a, b, c, d]);
    assert_eq!(doc.first_child(body), Some(a));
    assert_eq!(doc.last_child(body), Some(d));
    assert_eq!(doc.previous_sibling(c), Some(b));
    doc.check_invariants().unwrap();
}

#[test]
fn moving_a_node_detaches_it_first() {
    let mut doc = html();
    let [p, q, x] = ["p", "q", "x"].map(|n| doc.create_element(html_name(n)));
    doc.append_child(NodeId::DOCUMENT, p).unwrap();
    doc.append_child(p, x).unwrap();
    doc.append_child(p, q).unwrap();
    doc.append_child(q, x).unwrap();
    assert_eq!(children(&doc, p), [q]);
    assert_eq!(doc.parent(x), Some(q));
    doc.check_invariants().unwrap();
}

#[test]
fn detached_nodes_stay_usable() {
    let mut doc = html();
    let [p, a, b, c] = ["p", "a", "b", "c"].map(|n| doc.create_element(html_name(n)));
    for child in [a, b, c] {
        doc.append_child(p, child).unwrap();
    }
    doc.detach(b);
    assert_eq!(children(&doc, p), [a, c]);
    assert_eq!(doc.parent(b), None);
    assert_eq!(doc.next_sibling(a), Some(c));
    assert_eq!(doc.node_name(b), Some(&b"b"[..]));
    doc.detach(b);
    doc.check_invariants().unwrap();
}

#[test]
fn descendants_are_in_document_order_without_the_root() {
    let mut doc = html();
    let [a, b, c, d, e] = ["a", "b", "c", "d", "e"].map(|n| doc.create_element(html_name(n)));
    doc.append_child(NodeId::DOCUMENT, a).unwrap();
    doc.append_child(a, b).unwrap();
    doc.append_child(b, c).unwrap();
    doc.append_child(a, d).unwrap();
    doc.append_child(NodeId::DOCUMENT, e).unwrap();
    assert_eq!(doc.descendants(a).collect::<Vec<_>>(), [b, c, d]);
    assert_eq!(
        doc.descendants(NodeId::DOCUMENT).collect::<Vec<_>>(),
        [a, b, c, d, e]
    );
    assert_eq!(
        doc.ancestors(c).collect::<Vec<_>>(),
        [b, a, NodeId::DOCUMENT]
    );
}

#[test]
fn deep_trees_cost_no_stack() {
    // Deep enough to overflow a recursive walk; Miri is too slow for more
    let depth = if cfg!(miri) { 1_000 } else { 100_000 };
    let mut doc = html();
    let mut parent = NodeId::DOCUMENT;
    for _ in 0..depth {
        let child = doc.create_element(html_name("div"));
        doc.append_child(parent, child).unwrap();
        parent = child;
    }
    assert_eq!(doc.descendants(NodeId::DOCUMENT).count(), depth);
    assert_eq!(doc.ancestors(parent).count(), depth);
    assert_eq!(doc.path(parent).unwrap().len(), 4 * depth);
    let first = doc.first_child(NodeId::DOCUMENT).unwrap();
    assert_eq!(doc.document_order(parent, first), Some(Ordering::Greater));
}

#[test]
fn html4_documents_start_with_libxml2s_doctype() {
    let doc = Document::new_html4(None, None);
    let doctype = doc.doctype().unwrap();
    let NodeKind::Doctype(doctype) = doc.node_kind(doctype) else {
        panic!("not a doctype");
    };
    assert_eq!(
        doctype.public_id.as_deref(),
        Some(&b"-//W3C//DTD HTML 4.0 Transitional//EN"[..])
    );

    let doc = Document::new_html4(Some("about:legacy-compat".into()), None);
    let NodeKind::Doctype(doctype) = doc.node_kind(doc.doctype().unwrap()) else {
        panic!("not a doctype");
    };
    assert_eq!(doctype.public_id, None);
    assert_eq!(
        doctype.system_id.as_deref(),
        Some(&b"about:legacy-compat"[..])
    );
    assert_eq!(Document::new(DocumentKind::Html).doctype(), None);
}

#[test]
fn named_elements_take_the_documents_namespace() {
    let mut html = html();
    let div = html.create_element_named("Div");
    let NodeKind::Element(element) = html.node_kind(div) else {
        panic!("not an element");
    };
    assert_eq!(
        element.name,
        QualName::new(None, ns!(html), LocalName::from("Div"))
    );

    let mut xml = Document::new(DocumentKind::Xml);
    let div = xml.create_element_named("div");
    let NodeKind::Element(element) = xml.node_kind(div) else {
        panic!("not an element");
    };
    assert_eq!(element.name.ns, ns!());
}

#[test]
fn hierarchy_errors_leave_the_tree_unchanged() {
    let mut doc = html();
    let [a, b] = ["a", "b"].map(|n| doc.create_element(html_name(n)));
    let text = doc.create_text("t".into());
    let attribute = doc.create_attribute(html_name("href"), "x".into());
    doc.append_child(a, b).unwrap();

    assert_eq!(doc.append_child(b, a), Err(HierarchyError::Cycle));
    assert_eq!(doc.append_child(a, a), Err(HierarchyError::Cycle));
    assert_eq!(doc.append_child(text, b), Err(HierarchyError::NoChildren));
    assert_eq!(
        doc.append_child(a, NodeId::DOCUMENT),
        Err(HierarchyError::NotAChild)
    );
    assert_eq!(
        doc.append_child(a, attribute),
        Err(HierarchyError::NotAChild)
    );
    assert_eq!(doc.insert_before(a, text), Err(HierarchyError::NoParent));
    assert_eq!(doc.insert_after(b, a), Err(HierarchyError::Cycle));
    assert_eq!(children(&doc, a), [b]);
    doc.check_invariants().unwrap();
}

#[test]
fn attributes_belong_to_their_element_but_are_not_children() {
    let mut doc = html();
    let [a, b] = ["a", "b"].map(|n| doc.create_element(html_name(n)));
    let href = doc.create_attribute(QualName::new(None, ns!(), local_name!("href")), "/".into());
    doc.append_attribute(a, href).unwrap();
    assert_eq!(doc.attributes(a), [href]);
    assert_eq!(doc.parent(href), Some(a));
    assert_eq!(doc.children(a).count(), 0);

    doc.append_attribute(b, href).unwrap();
    assert_eq!(doc.attributes(a), []);
    assert_eq!(doc.attributes(b), [href]);
    doc.detach(href);
    assert_eq!(doc.attributes(b), []);
    assert_eq!(
        doc.append_attribute(href, a),
        Err(HierarchyError::NotAnAttribute)
    );
    doc.check_invariants().unwrap();
}

#[test]
fn character_data_keeps_bytes_that_are_not_utf8() {
    let mut doc = html();
    let text = doc.create_text(b"caf\xe9".to_vec());
    let NodeKind::Text(content) = doc.node_kind(text) else {
        panic!("not a text node");
    };
    assert_eq!(content, b"caf\xe9");
}

#[test]
fn heap_size_counts_nodes_and_their_content() {
    let mut doc = html();
    let empty = doc.heap_size();
    for _ in 0..100 {
        doc.create_text(vec![b'x'; 1000]);
    }
    assert!(doc.heap_size() >= empty + 100 * 1000);
}

#[test]
fn invariants_hold_on_a_wide_tree_in_linear_time() {
    // Quadratic checks would take minutes here
    let width = if cfg!(miri) { 1_000 } else { 200_000 };
    let mut doc = html();
    let parent = doc.create_element(html_name("ul"));
    doc.append_child(NodeId::DOCUMENT, parent).unwrap();
    for _ in 0..width {
        let child = doc.create_element(html_name("li"));
        doc.append_child(parent, child).unwrap();
    }
    doc.check_invariants().unwrap();
}

fn corrupted(corrupt: impl FnOnce(&mut Document, [NodeId; 3])) -> String {
    let mut doc = html();
    let [a, b, c] = ["a", "b", "c"].map(|n| doc.create_element(html_name(n)));
    doc.append_child(NodeId::DOCUMENT, a).unwrap();
    doc.append_child(a, b).unwrap();
    doc.append_child(a, c).unwrap();
    doc.check_invariants().unwrap();
    corrupt(&mut doc, [a, b, c]);
    doc.check_invariants().unwrap_err()
}

#[test]
fn invariants_catch_broken_links() {
    let error = corrupted(|doc, [_, b, _]| doc.node_mut(b).previous_sibling = Some(b));
    assert!(error.contains("wrong previous sibling"), "{error}");

    let error = corrupted(|doc, [a, b, _]| doc.node_mut(a).last_child = Some(b));
    assert!(error.contains("wrong last child"), "{error}");

    // Children whose parent has lost them
    let error = corrupted(|doc, [a, _, _]| {
        let a = doc.node_mut(a);
        a.first_child = None;
        a.last_child = None;
    });
    assert!(error.contains("isn't listed"), "{error}");
}

#[test]
fn invariants_catch_cycles() {
    // Siblings that loop back to the first child
    let error = corrupted(|doc, [_, b, c]| doc.node_mut(c).next_sibling = Some(b));
    assert!(error.contains("listed twice"), "{error}");

    // Two detached nodes that are each other's parent and only child
    let error = corrupted(|doc, [_, b, c]| {
        doc.detach(b);
        doc.detach(c);
        for (node, other) in [(b, c), (c, b)] {
            let node = doc.node_mut(node);
            node.parent = Some(other);
            node.first_child = Some(other);
            node.last_child = Some(other);
        }
    });
    assert!(error.contains("its own ancestor"), "{error}");
}

#[test]
fn invariants_catch_attributes_listed_twice() {
    let error = corrupted(|doc, [a, _, _]| {
        let href = doc.create_attribute(html_name("href"), b"/".to_vec());
        doc.append_attribute(a, href).unwrap();
        let NodeKind::Element(element) = &mut doc.node_mut(a).kind else {
            unreachable!()
        };
        element.attributes.push(href);
    });
    assert!(error.contains("listed twice"), "{error}");
}

fn attribute(doc: &mut Document, element: NodeId, name: QualName, value: &str) -> NodeId {
    let attribute = doc.create_attribute(name, value.into());
    doc.append_attribute(element, attribute).unwrap();
    attribute
}

fn plain(local: &str) -> QualName {
    QualName::new(None, ns!(), LocalName::from(local))
}

fn xlink(local: &str) -> QualName {
    QualName::new(
        Some(Prefix::from("xlink")),
        ns!(xlink),
        LocalName::from(local),
    )
}

/// `<p id class>a<b/><!--c-->d<i/></p>` under the document.
fn paragraph() -> (Document, NodeId, [NodeId; 2], [NodeId; 5]) {
    let mut doc = html();
    let p = doc.create_element(html_name("p"));
    doc.append_child(NodeId::DOCUMENT, p).unwrap();
    let id = attribute(&mut doc, p, plain("id"), "x");
    let class = attribute(&mut doc, p, plain("class"), "y");
    let a = doc.create_text("a".into());
    let b = doc.create_element(html_name("b"));
    let c = doc.create_comment("c".into());
    let d = doc.create_text("d".into());
    let i = doc.create_element(html_name("i"));
    for child in [a, b, c, d, i] {
        doc.append_child(p, child).unwrap();
    }
    (doc, p, [id, class], [a, b, c, d, i])
}

#[test]
fn attributes_are_siblings_of_each_other_only() {
    let (doc, _, [id, class], [a, ..]) = paragraph();
    assert_eq!(doc.next_in_parent(id), Some(class));
    assert_eq!(doc.previous_in_parent(class), Some(id));
    assert_eq!(doc.previous_in_parent(id), None);
    assert_eq!(doc.next_in_parent(class), None);
    assert_eq!(doc.next_element_sibling(id), None);
    assert_eq!(doc.next_in_parent(a), doc.next_sibling(a));
}

#[test]
fn element_walks_skip_other_nodes() {
    let (doc, p, _, [a, b, c, d, i]) = paragraph();
    assert_eq!(doc.next_element_sibling(a), Some(b));
    assert_eq!(doc.next_element_sibling(b), Some(i));
    assert_eq!(doc.previous_element_sibling(d), Some(b));
    assert_eq!(doc.previous_element_sibling(b), None);
    assert_eq!(doc.next_element_sibling(c), Some(i));
    assert_eq!(doc.element_children(p).collect::<Vec<_>>(), [b, i]);
    assert_eq!(doc.last_element_child(p), Some(i));
    assert_eq!(doc.last_element_child(a), None);
}

#[test]
fn document_order_puts_attributes_between_element_and_children() {
    let (mut doc, p, [id, class], [a, b, .., i]) = paragraph();
    assert_eq!(doc.document_order(a, a), Some(Ordering::Equal));
    assert_eq!(doc.document_order(a, b), Some(Ordering::Less));
    assert_eq!(doc.document_order(i, b), Some(Ordering::Greater));
    assert_eq!(doc.document_order(p, b), Some(Ordering::Less));
    assert_eq!(
        doc.document_order(NodeId::DOCUMENT, id),
        Some(Ordering::Less)
    );
    assert_eq!(doc.document_order(id, p), Some(Ordering::Greater));
    assert_eq!(doc.document_order(id, a), Some(Ordering::Less));
    assert_eq!(doc.document_order(class, id), Some(Ordering::Greater));
    let detached = doc.create_element(html_name("div"));
    assert_eq!(doc.document_order(detached, p), None);
    doc.detach(class);
    assert_eq!(doc.document_order(class, p), None);
}

#[test]
fn paths_number_steps_that_share_a_name() {
    let (mut doc, p, [id, _], [a, b, c, d, _]) = paragraph();
    let p2 = doc.create_element(html_name("p"));
    doc.append_child(NodeId::DOCUMENT, p2).unwrap();
    let path = |doc: &Document, id| String::from_utf8(doc.path(id).unwrap()).unwrap();
    assert_eq!(path(&doc, NodeId::DOCUMENT), "/");
    assert_eq!(path(&doc, p), "/p[1]");
    assert_eq!(path(&doc, p2), "/p[2]");
    assert_eq!(path(&doc, id), "/p[1]/@id");
    assert_eq!(path(&doc, a), "/p[1]/text()[1]");
    assert_eq!(path(&doc, b), "/p[1]/b");
    assert_eq!(path(&doc, c), "/p[1]/comment()");
    assert_eq!(path(&doc, d), "/p[1]/text()[2]");

    let fragment = doc.create_fragment();
    let span = doc.create_element(html_name("span"));
    doc.append_child(fragment, span).unwrap();
    assert_eq!(doc.path(span), None);
    let doctype = doc.create_doctype(Doctype {
        name: "html".into(),
        public_id: None,
        system_id: None,
    });
    assert_eq!(doc.path(doctype), None);
}

#[test]
fn paths_name_foreign_elements_by_prefix() {
    let mut doc = html();
    let svg_name = |local: &str| QualName::new(None, ns!(svg), LocalName::from(local));
    let svg = doc.create_element(svg_name("svg"));
    doc.append_child(NodeId::DOCUMENT, svg).unwrap();
    let [g, prefixed_g] = ["g", "svg:g"].map(|local| doc.create_element(svg_name(local)));
    doc.append_child(svg, g).unwrap();
    doc.append_child(svg, prefixed_g).unwrap();
    let href = attribute(&mut doc, g, xlink("href"), "#");
    let long = doc.create_element(svg_name(&"x".repeat(200)));
    doc.append_child(svg, long).unwrap();

    assert_eq!(doc.path(prefixed_g).unwrap(), b"/svg:svg/svg:svg:g");
    assert_eq!(doc.path(href).unwrap(), b"/svg:svg/svg:g/@xlink:href");
    assert_eq!(doc.path(long).unwrap().len(), "/svg:svg/".len() + 98);
}

#[test]
fn attribute_lookup_follows_libxml2() {
    let mut doc = html();
    let p = doc.create_element(html_name("p"));
    doc.append_child(NodeId::DOCUMENT, p).unwrap();
    let a = doc.create_element(html_name("a"));
    doc.append_child(p, a).unwrap();
    let id = attribute(&mut doc, a, plain("id"), "x");
    let href = attribute(&mut doc, a, xlink("href"), "#");
    let literal = attribute(&mut doc, a, plain("xlink:title"), "t");

    assert_eq!(doc.attribute_named(a, b"href"), Some(href));
    assert_eq!(doc.attribute_named(a, b"xlink:title"), Some(literal));
    assert_eq!(doc.attribute_in_namespace(a, b"href", None), None);
    assert_eq!(
        doc.attribute_in_namespace(a, b"href", Some(ns!(xlink).as_bytes())),
        Some(href)
    );
    assert_eq!(doc.attribute_in_namespace(a, b"id", None), Some(id));
    assert_eq!(doc.attribute_value(a, b"id"), Some(&b"x"[..]));
    assert_eq!(doc.attribute_value(a, b"href"), None);
    // An unbound prefix is part of the name
    assert_eq!(doc.attribute_value(a, b"xlink:href"), None);
    assert_eq!(doc.attribute_value(a, b"xlink:title"), Some(&b"t"[..]));

    doc.define_namespace(p, Some(Prefix::from("xlink")), ns!(xlink));
    assert_eq!(doc.attribute_value(a, b"xlink:href"), Some(&b"#"[..]));
    assert_eq!(doc.attribute_value(a, b"xlink:title"), None);
    assert_eq!(doc.attribute_value(id, b"id"), None);
}

#[test]
fn lang_comes_from_the_nearest_xml_lang() {
    let mut doc = html();
    let [outer, inner] = ["div", "span"].map(|n| doc.create_element(html_name(n)));
    doc.append_child(NodeId::DOCUMENT, outer).unwrap();
    doc.append_child(outer, inner).unwrap();
    attribute(&mut doc, inner, plain("lang"), "en");
    assert_eq!(doc.lang(inner), None);
    let xml_lang = QualName::new(Some(Prefix::from("xml")), ns!(xml), local_name!("lang"));
    attribute(&mut doc, outer, xml_lang, "fr");
    assert_eq!(doc.lang(inner), Some(&b"fr"[..]));
}

#[test]
fn namespace_scopes_take_the_nearest_definition_of_each_prefix() {
    let mut doc = html();
    let [outer, inner] = ["div", "span"].map(|n| doc.create_element(html_name(n)));
    doc.append_child(NodeId::DOCUMENT, outer).unwrap();
    doc.append_child(outer, inner).unwrap();
    let text = doc.create_text("t".into());
    doc.append_child(inner, text).unwrap();
    doc.define_namespace(outer, Some(Prefix::from("a")), Namespace::from("urn:outer"));
    doc.define_namespace(outer, Some(Prefix::from("b")), Namespace::from("urn:b"));
    doc.define_namespace(inner, Some(Prefix::from("a")), Namespace::from("urn:inner"));
    // A second definition of a prefix on the same element is ignored
    doc.define_namespace(inner, Some(Prefix::from("a")), Namespace::from("urn:again"));

    let scopes = |id| {
        doc.namespace_scopes(id)
            .iter()
            .map(|ns| (ns.prefix.unwrap().to_owned(), ns.href.to_owned()))
            .collect::<Vec<_>>()
    };
    let pair = |prefix: &str, href: &str| (prefix.to_owned(), href.to_owned());
    assert_eq!(scopes(text), [pair("a", "urn:inner"), pair("b", "urn:b")]);
    assert_eq!(scopes(outer), [pair("a", "urn:outer"), pair("b", "urn:b")]);
    assert_eq!(
        doc.search_namespace(text, b"a").map(|ns| ns.href),
        Some("urn:inner")
    );
    assert_eq!(
        doc.search_namespace(text, b"xml").map(|ns| ns.href),
        Some("http://www.w3.org/XML/1998/namespace")
    );
    assert_eq!(doc.search_namespace(text, b"c"), None);
}

#[test]
fn an_ancestors_own_namespace_is_in_scope() {
    let mut doc = html();
    let svg = doc.create_element(QualName::new(None, ns!(svg), local_name!("svg")));
    let g = doc.create_element(QualName::new(None, ns!(svg), local_name!("g")));
    doc.append_child(svg, g).unwrap();
    assert_eq!(
        doc.search_namespace(g, b"svg").map(|ns| ns.href),
        Some(&*ns!(svg))
    );
    assert_eq!(doc.search_namespace(svg, b"svg"), None);
}

#[test]
fn blank_nodes_are_whitespace_text() {
    let mut doc = html();
    let blank = doc.create_text(" \t\r\n".into());
    let empty = doc.create_cdata(Vec::new());
    let form_feed = doc.create_text("\x0c".into());
    let comment = doc.create_comment(" ".into());
    assert!(doc.is_blank(blank));
    assert!(doc.is_blank(empty));
    assert!(!doc.is_blank(form_feed));
    assert!(!doc.is_blank(comment));
    assert!(!doc.is_blank(NodeId::DOCUMENT));
}

#[test]
fn special_characters_are_escaped_as_libxml2_does() {
    assert_eq!(
        encode_special_chars(b"<a href=\"x\">'&'\r\n\xff</a>"),
        b"&lt;a href=&quot;x&quot;&gt;'&amp;'&#13;\n\xff&lt;/a&gt;"
    );
}
