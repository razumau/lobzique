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

fn text(doc: &Document, id: NodeId) -> &[u8] {
    match doc.node_kind(id) {
        NodeKind::Text(text) => text,
        kind => panic!("{kind:?} isn't text"),
    }
}

#[test]
fn reparenting_merges_text_only_where_libxml2_does() {
    let (mut doc, p, _, [a, b, _, d, i]) = paragraph();
    doc.detach(i);
    let x = doc.create_text("x".into());
    let merged = doc.reparent(p, Position::LastChild, x, false).unwrap();
    assert_eq!((merged, text(&doc, d)), (d, &b"dx"[..]));
    assert_eq!(doc.parent(x), None, "the original is left behind");

    let y = doc.create_text("y".into());
    let copy = doc.reparent(b, Position::Before, y, false).unwrap();
    assert_ne!(copy, y);
    assert_eq!(children(&doc, p)[..3], [a, copy, b]);
    assert_eq!(text(&doc, a), b"a");
    doc.check_invariants().unwrap();
}

#[test]
fn replacing_with_text_merges_both_neighbors_into_the_previous_one() {
    let mut doc = html();
    let p = doc.create_element(html_name("p"));
    let [a, b, c] = [b"a", b"b", b"c"].map(|t| doc.create_text(t.to_vec()));
    let br = doc.create_element(html_name("br"));
    for child in [a, br, c] {
        doc.append_child(p, child).unwrap();
    }
    let merged = doc.reparent(br, Position::Replace, b, false).unwrap();
    assert_eq!(merged, a);
    assert_eq!(children(&doc, p), [a]);
    assert_eq!(text(&doc, a), b"abc");
    assert_eq!((doc.parent(c), text(&doc, c)), (None, &b"c"[..]));
    assert_eq!(doc.parent(br), None);
    doc.check_invariants().unwrap();
}

#[test]
fn refused_reparenting_follows_nokogiri() {
    let (mut doc, p, [id, _], [a, b, ..]) = paragraph();
    let snapshot = |doc: &Document| children(doc, p);
    let before = snapshot(&doc);
    let error = |doc: &mut Document, pivot, position, node| {
        doc.reparent(pivot, position, node, false).unwrap_err()
    };
    assert_eq!(
        error(&mut doc, a, Position::LastChild, b),
        ReparentError::NotAllowed
    );
    assert_eq!(
        error(&mut doc, p, Position::LastChild, id),
        ReparentError::NotAllowed
    );
    let fragment = doc.create_fragment();
    assert_eq!(
        error(&mut doc, p, Position::LastChild, fragment),
        ReparentError::NotAllowed
    );
    assert_eq!(
        error(&mut doc, b, Position::LastChild, p),
        ReparentError::Cycle
    );
    assert_eq!(error(&mut doc, b, Position::After, p), ReparentError::Cycle);
    assert_eq!(snapshot(&doc), before, "nothing changed");

    assert_eq!(
        error(&mut doc, b, Position::After, b),
        ReparentError::Refused
    );
    assert_eq!(doc.parent(b), None, "unlinked first, as in Nokogiri");
    let loose = doc.create_element(html_name("q"));
    assert_eq!(
        error(&mut doc, loose, Position::Before, a),
        ReparentError::Refused
    );
    doc.check_invariants().unwrap();
}

#[test]
fn nodes_from_another_document_are_copied_in() {
    let (mut source, p, _, [_, b, ..]) = paragraph();
    let mut doc = html();
    let body = doc.create_element(html_name("body"));
    doc.append_child(NodeId::DOCUMENT, body).unwrap();
    let copy = doc
        .reparent_from(body, Position::LastChild, &mut source, p, false)
        .unwrap();
    assert_eq!(doc.parent(copy), Some(body));
    assert_eq!(source.parent(p), None);
    assert_eq!(source.parent(b), Some(p), "the original keeps its subtree");
    assert_eq!(doc.attributes(copy).len(), 2);
    assert_eq!(doc.content(copy), source.content(p));
    doc.check_invariants().unwrap();
    source.check_invariants().unwrap();
}

#[test]
fn copies_take_what_their_depth_says() {
    let (mut doc, p, _, _) = paragraph();
    let deep = doc.copy(p, CopyDepth::Deep).unwrap();
    let attributes = doc.copy(p, CopyDepth::Attributes).unwrap();
    let shallow = doc.copy(p, CopyDepth::Shallow).unwrap();
    assert_eq!(children(&doc, deep).len(), 5);
    assert_eq!(doc.content(deep), doc.content(p));
    assert_eq!(
        (
            doc.attributes(attributes).len(),
            doc.first_child(attributes)
        ),
        (2, None)
    );
    assert_eq!(
        (doc.attributes(shallow).len(), doc.first_child(shallow)),
        (0, None)
    );
    assert_eq!(doc.parent(deep), None);
    assert_eq!(doc.copy(NodeId::DOCUMENT, CopyDepth::Deep), None);
    doc.check_invariants().unwrap();
}

#[test]
fn deep_copies_cost_no_stack() {
    let depth = if cfg!(miri) { 1_000 } else { 100_000 };
    let mut doc = html();
    let top = doc.create_element(html_name("div"));
    let mut parent = top;
    for _ in 0..depth {
        let child = doc.create_element(html_name("div"));
        doc.append_child(parent, child).unwrap();
        parent = child;
    }
    let copy = doc.copy(top, CopyDepth::Deep).unwrap();
    assert_eq!(doc.descendants(copy).count(), depth);
    let mut other = html();
    let body = other.create_element(html_name("body"));
    other.append_child(NodeId::DOCUMENT, body).unwrap();
    other
        .reparent_from(body, Position::LastChild, &mut doc, top, false)
        .unwrap();
    assert_eq!(other.descendants(body).count(), depth + 1);
}

#[test]
fn a_document_copy_leaves_unlinked_nodes_behind() {
    let (mut doc, p, _, [a, ..]) = paragraph();
    doc.set_encoding(Some(b"UTF-8".to_vec()));
    doc.detach(a);
    let doctype = doc.create_internal_subset(Doctype {
        name: b"html".to_vec(),
        public_id: None,
        system_id: None,
    });
    assert!(doctype.is_some());
    let copy = doc.copy_document(true);
    assert_eq!(copy.node_ids().len(), 1 + 1 + 1 + 2 + 4);
    assert_eq!(copy.encoding(), Some(&b"UTF-8"[..]));
    assert!(matches!(
        copy.node_kind(copy.doctype().unwrap()),
        NodeKind::Doctype(_)
    ));
    assert_eq!(copy.content(NodeId::DOCUMENT), doc.content(p));
    assert_eq!(doc.copy_document(false).node_ids().len(), 1);
    copy.check_invariants().unwrap();
}

#[test]
fn copies_define_the_foreign_namespaces_they_use() {
    let mut doc = html();
    let html = doc.create_element(html_name("html"));
    let svg = doc.create_element(QualName::new(None, ns!(svg), local_name!("svg")));
    let a = doc.create_element(QualName::new(None, ns!(svg), local_name!("a")));
    doc.append_child(NodeId::DOCUMENT, html).unwrap();
    doc.append_child(html, svg).unwrap();
    doc.append_child(svg, a).unwrap();
    attribute(&mut doc, a, xlink("href"), "h");
    doc.define_namespace(html, Some(Prefix::from("svg")), ns!(svg));
    doc.define_namespace(html, Some(Prefix::from("xlink")), ns!(xlink));

    let copy = doc.copy(svg, CopyDepth::Deep).unwrap();
    let prefixes = |doc: &Document, id| -> Vec<String> {
        doc.namespace_definitions(id)
            .iter()
            .map(|definition| definition.prefix.as_deref().unwrap_or("").to_owned())
            .collect()
    };
    assert_eq!(prefixes(&doc, copy), ["svg", "xlink"]);
    let a_copy = doc.first_child(copy).unwrap();
    assert!(prefixes(&doc, a_copy).is_empty());

    // Moving it back under definitions already in scope drops them
    doc.reparent(html, Position::LastChild, copy, false)
        .unwrap();
    assert!(prefixes(&doc, copy).is_empty());
}

#[test]
fn prefixed_names_resolve_when_reparented() {
    let mut doc = html();
    let svg = doc.create_element(QualName::new(None, ns!(svg), local_name!("svg")));
    let circle = doc.create_element_named("svg:circle");
    let unknown = doc.create_element_named("x:y");
    doc.reparent(svg, Position::LastChild, circle, false)
        .unwrap();
    doc.reparent(svg, Position::LastChild, unknown, false)
        .unwrap();
    assert_eq!(doc.node_name(circle), Some(&b"circle"[..]));
    assert_eq!(doc.namespace(circle).map(|ns| ns.href), Some(&*ns!(svg)));
    assert_eq!(doc.node_name(unknown), Some(&b"x:y"[..]));
}

#[test]
fn namespace_inheritance_takes_the_parents_namespace() {
    let mut doc = html();
    let svg = doc.create_element(QualName::new(None, ns!(svg), local_name!("svg")));
    let [kept, inherits] = ["g", "g"].map(|name| doc.create_element_named(name));
    doc.reparent(svg, Position::LastChild, kept, false).unwrap();
    doc.reparent(svg, Position::LastChild, inherits, true)
        .unwrap();
    assert_eq!(doc.namespace(kept), None);
    assert_eq!(doc.namespace(inherits).map(|ns| ns.href), Some(&*ns!(svg)));
}

#[test]
fn setting_attributes_follows_xml_set_prop() {
    let (mut doc, p, [id, _], _) = paragraph();
    assert_eq!(doc.set_attribute(p, "id", b"new".to_vec()), Some(id));
    assert_eq!(doc.attribute_value(p, b"id"), Some(&b"new"[..]));
    let lang = doc.set_attribute(p, "xml:lang", b"de".to_vec()).unwrap();
    assert_eq!(doc.namespace(lang).map(|ns| ns.href), Some(&*ns!(xml)));
    assert_eq!(doc.lang(p), Some(&b"de"[..]));
    doc.set_lang(p, b"fr".to_vec());
    assert_eq!(
        (doc.lang(p), doc.attributes(p).len()),
        (Some(&b"fr"[..]), 3)
    );
    let unbound = doc.set_attribute(p, "a:b", Vec::new()).unwrap();
    assert_eq!(
        (doc.node_name(unbound), doc.namespace(unbound)),
        (Some(&b"a:b"[..]), None)
    );
    let text = doc.create_text("t".into());
    assert_eq!(doc.set_attribute(text, "id", Vec::new()), None);
    doc.check_invariants().unwrap();
}

#[test]
fn setting_content_follows_xml_node_set_content() {
    let (mut doc, p, [id, _], [a, b, c, ..]) = paragraph();
    doc.set_content(p, b"&lt;&#65;&#x42;&amp;amp;&#0;&nbsp;&#xZZ;&".to_vec());
    let [only] = children(&doc, p)[..] else {
        panic!("one text child")
    };
    assert_eq!(text(&doc, only), b"<AB&amp;&#0;&nbsp;&#xZZ;&");
    assert_eq!((doc.parent(a), doc.parent(b)), (None, None));
    doc.set_content(p, Vec::new());
    assert_eq!(doc.first_child(p), None);

    doc.set_content(c, b"&lt;".to_vec());
    assert_eq!(
        doc.content(c),
        Some(b"&lt;".to_vec()),
        "comments take content as is"
    );
    doc.set_content(id, b"&quot;".to_vec());
    assert_eq!(doc.content(id), Some(b"\"".to_vec()));
    doc.set_content(NodeId::DOCUMENT, b"x".to_vec());
    assert_eq!(doc.first_child(NodeId::DOCUMENT), None);
    doc.check_invariants().unwrap();
}

#[test]
fn renaming_keeps_fixed_names() {
    let (mut doc, p, [id, _], [a, ..]) = paragraph();
    doc.set_name(p, "section");
    doc.set_name(id, "key");
    doc.set_name(a, "other");
    assert_eq!(doc.node_name(p), Some(&b"section"[..]));
    assert_eq!(doc.node_name(id), Some(&b"key"[..]));
    assert_eq!(doc.node_name(a), Some(&b"text"[..]));
    assert_eq!(doc.namespace(p), None);
}

#[test]
fn roots_and_internal_subsets() {
    let (mut doc, p, _, [a, ..]) = paragraph();
    let q = doc.create_element(html_name("q"));
    assert_eq!(doc.set_root(Some(a)), Err(HierarchyError::NotAnElement));
    assert_eq!(
        doc.parent(p),
        Some(NodeId::DOCUMENT),
        "a refused root changes nothing"
    );
    doc.set_root(Some(q)).unwrap();
    assert_eq!(
        (doc.parent(p), doc.parent(q)),
        (None, Some(NodeId::DOCUMENT))
    );
    doc.set_root(None).unwrap();
    assert_eq!(doc.first_child(NodeId::DOCUMENT), None);

    let mut xml = Document::new(DocumentKind::Xml);
    let comment = xml.create_comment("c".into());
    let root = xml.create_element_named("r");
    xml.append_child(NodeId::DOCUMENT, comment).unwrap();
    xml.set_root(Some(root)).unwrap();
    let doctype = Doctype {
        name: b"r".to_vec(),
        public_id: None,
        system_id: None,
    };
    let dtd = xml.create_internal_subset(doctype.clone()).unwrap();
    assert_eq!(children(&xml, NodeId::DOCUMENT), [comment, dtd, root]);
    assert_eq!(xml.create_internal_subset(doctype), None);
    xml.check_invariants().unwrap();
}

#[test]
fn names_with_an_empty_side_of_the_colon_are_not_qualified() {
    let mut doc = html();
    let a = doc.create_element(html_name("a"));
    doc.append_child(NodeId::DOCUMENT, a).unwrap();
    doc.define_namespace(a, Some(Prefix::from("xlink")), ns!(xlink));
    for name in ["xlink:", ":href"] {
        let attribute = doc.set_attribute(a, name, b"v".to_vec()).unwrap();
        let NodeKind::Attribute(attribute) = doc.node_kind(attribute) else {
            unreachable!()
        };
        assert_eq!(attribute.name, plain(name), "{name}");
    }

    // Reparenting resolves prefixes the same way
    let svg = doc.create_element(QualName::new(None, ns!(svg), local_name!("svg")));
    let g = doc.create_element(QualName::new(None, ns!(svg), local_name!("g")));
    doc.append_child(svg, g).unwrap();
    let trailing = attribute(&mut doc, g, plain("xlink:"), "y");
    doc.reparent(a, Position::LastChild, svg, false).unwrap();
    let NodeKind::Attribute(attribute) = doc.node_kind(trailing) else {
        unreachable!()
    };
    assert_eq!(attribute.name, plain("xlink:"));
    doc.check_invariants().unwrap();
}

#[test]
fn decoding_references_is_linear() {
    // Quadratic decoding would take minutes here
    let length = if cfg!(miri) { 1_000 } else { 1_000_000 };
    let mut doc = html();
    let p = doc.create_element(html_name("p"));
    let mut content = vec![b'&'; length];
    content.extend_from_slice(b"amp;");
    doc.set_content(p, content);
    assert_eq!(doc.content(p).map(|content| content.len()), Some(length));
}

#[test]
fn references_end_at_the_first_semicolon_before_another_ampersand() {
    let mut doc = html();
    let p = doc.create_element(html_name("p"));
    doc.set_content(p, b"&am&amp;&#65&#66;&;&".to_vec());
    assert_eq!(doc.content(p), Some(b"&am&&#65B&;&".to_vec()));
}

#[test]
fn text_put_next_to_itself_is_left_detached_as_a_copy() {
    for position in [Position::Before, Position::After] {
        let (mut doc, p, _, [a, b, ..]) = paragraph();
        let copy = doc.reparent(a, position, a, false).unwrap();
        assert_ne!(copy, a);
        assert_eq!((doc.parent(a), doc.parent(copy)), (None, None));
        assert_eq!(text(&doc, copy), b"a");
        assert_eq!(doc.first_child(p), Some(b));
        doc.check_invariants().unwrap();
    }
}

#[test]
fn document_order_from_walking_and_numbering_agree() {
    let (mut doc, _, [id, _], [_, b, ..]) = paragraph();
    let inner = doc.create_element(html_name("em"));
    doc.append_child(b, inner).unwrap();
    attribute(&mut doc, inner, plain("title"), "t");
    let loose = doc.create_element(html_name("q"));
    let loose_child = doc.create_text("q".into());
    doc.append_child(loose, loose_child).unwrap();
    doc.create_attribute(plain("x"), Vec::new());
    doc.detach(id);

    let numbers = doc.number_nodes();
    for a in doc.node_ids() {
        for b in doc.node_ids() {
            let (a_number, b_number) = (numbers[a.index()], numbers[b.index()]);
            let numbered = (a_number.0 == b_number.0).then(|| a_number.1.cmp(&b_number.1));
            assert_eq!(doc.walk_order(a, b).0, numbered, "{a:?} {b:?}");
        }
    }
}

#[test]
fn sorting_siblings_in_document_order_is_not_quadratic() {
    // Comparing by walking siblings alone would take minutes here
    let width = if cfg!(miri) { 300 } else { 200_000 };
    let mut doc = html();
    let parent = doc.create_element(html_name("ul"));
    doc.append_child(NodeId::DOCUMENT, parent).unwrap();
    for _ in 0..width {
        let child = doc.create_element(html_name("li"));
        doc.append_child(parent, child).unwrap();
    }
    let in_order: Vec<_> = doc.children(parent).collect();
    // A permutation, since 99991 is a prime that doesn't divide the width
    let mut items: Vec<_> = (0..width).map(|i| in_order[i * 99_991 % width]).collect();
    items.sort_by(|&a, &b| doc.document_order(a, b).unwrap());
    assert_eq!(items, in_order);
    assert!(doc.order.get().is_some(), "comparisons numbered the nodes");
    doc.check_invariants().unwrap();

    doc.detach(in_order[0]);
    assert!(doc.order.get().is_none(), "a change drops the numbers");
    assert_eq!(doc.document_order(in_order[0], in_order[1]), None);
}

#[test]
fn invariants_catch_stale_document_order() {
    let error = corrupted(|doc, [a, b, c]| {
        doc.order.get_or_init(|| doc.number_nodes());
        // Swapped without going through link, which would drop the numbers
        doc.node_mut(a).first_child = Some(c);
        doc.node_mut(a).last_child = Some(b);
        doc.node_mut(c).previous_sibling = None;
        doc.node_mut(c).next_sibling = Some(b);
        doc.node_mut(b).previous_sibling = Some(c);
        doc.node_mut(b).next_sibling = None;
    });
    assert!(error.contains("stale"), "{error}");
}

#[test]
fn copies_write_no_namespace_as_their_new_document_does() {
    let mut xml = Document::new(DocumentKind::Xml);
    let root = xml.create_element_named("div");
    let child = xml.create_element_named("span");
    xml.append_child(root, child).unwrap();
    let svg = xml.create_element(QualName::new(None, ns!(svg), local_name!("svg")));
    xml.append_child(root, svg).unwrap();

    let mut doc = html();
    let copy = doc.import(&xml, root, CopyDepth::Deep).unwrap();
    let names: Vec<_> = std::iter::once(copy)
        .chain(doc.descendants(copy))
        .map(|id| match doc.node_kind(id) {
            NodeKind::Element(element) => element.name.ns.clone(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(names, [ns!(html), ns!(html), ns!(svg)]);
    doc.check_invariants().unwrap();

    let back = xml.import(&doc, copy, CopyDepth::Attributes).unwrap();
    let NodeKind::Element(element) = xml.node_kind(back) else {
        unreachable!()
    };
    assert_eq!(element.name.ns, ns!());
    xml.check_invariants().unwrap();
}

#[test]
fn invariants_catch_elements_in_the_other_kind_of_documents_no_namespace() {
    let mut doc = html();
    doc.create_element(plain("div"));
    let error = doc.check_invariants().unwrap_err();
    assert!(error.contains("not this document's"), "{error}");
}

#[test]
fn invariants_catch_prefixes_without_a_local_name() {
    let mut doc = html();
    let a = doc.create_element(html_name("a"));
    attribute(&mut doc, a, xlink(""), "v");
    let error = doc.check_invariants().unwrap_err();
    assert!(error.contains("no local name"), "{error}");
}
