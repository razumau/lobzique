use super::*;
use html5ever::{local_name, ns};

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
    assert_eq!(doc.node_name(element), Some("Div"));
    assert_eq!(doc.node_name(text), Some("text"));
    assert_eq!(doc.node_name(comment), Some("comment"));
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
    assert_eq!(doc.node_name(b), Some("b"));
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
        Some("-//W3C//DTD HTML 4.0 Transitional//EN")
    );

    let doc = Document::new_html4(Some("about:legacy-compat".into()), None);
    let NodeKind::Doctype(doctype) = doc.node_kind(doc.doctype().unwrap()) else {
        panic!("not a doctype");
    };
    assert_eq!(doctype.public_id, None);
    assert_eq!(doctype.system_id.as_deref(), Some("about:legacy-compat"));
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
