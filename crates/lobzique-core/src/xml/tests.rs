use super::save_options::*;
use super::*;
use crate::dom::{Doctype, Document, DocumentKind, NodeId, NodeKind};
use crate::html5::{parse_document, Options};

fn parse(input: &str) -> Document {
    parse_document(input.as_bytes(), &Options::default())
        .unwrap()
        .0
}

fn string(document: &Document, id: NodeId, encoding: Option<&str>, options: u32) -> String {
    let output = write(document, id, encoding.map(str::as_bytes), b"  ", options).unwrap();
    String::from_utf8(output.bytes).unwrap()
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
fn html_output_is_libxml2s_html_serializer() {
    let html = parse("<p>");
    let p = find(&html, "p");
    assert_eq!(write(&html, p, None, b"", FORMAT), Err(HtmlUnsupported));
    assert_eq!(
        write(&html, p, None, b"", AS_XML | AS_HTML),
        Err(HtmlUnsupported)
    );
    assert!(write(&html, p, None, b"", AS_XHTML | AS_HTML).is_ok());
    assert_eq!(
        write(&html, NodeId::DOCUMENT, None, b"", AS_XHTML | AS_HTML),
        Err(HtmlUnsupported)
    );
    let xml = Document::new(DocumentKind::Xml);
    assert!(write(&xml, NodeId::DOCUMENT, None, b"", 0).is_ok());
}

#[test]
fn text_without_an_encoding_is_ascii() {
    let mut document = parse("<p title='\u{e9}\u{e9}'>\u{e9}\u{1f600}\u{1}\r\x7f</p>");
    let p = find(&document, "p");
    assert_eq!(
        string(&document, p, None, AS_XML),
        "<p title=\"&#xE9;&#xE9;\">&#xE9;&#x1F600;&#xFFFD;\n\x7f</p>"
    );
    document.set_encoding(Some(b"UTF-8".to_vec()));
    // the parse turned CR into LF; attributes stay UTF-8 when the document has an encoding
    assert_eq!(
        string(&document, p, None, AS_XML),
        "<p title=\"\u{e9}\u{e9}\">&#xE9;&#x1F600;&#xFFFD;\n\x7f</p>"
    );
    assert_eq!(
        string(&document, p, Some("UTF-8"), AS_XML),
        "<p title=\"\u{e9}\u{e9}\">\u{e9}\u{1f600}\u{1}\n\x7f</p>"
    );
}

#[test]
fn malformed_utf8_is_a_replacement_character() {
    let mut document = Document::new(DocumentKind::Xml);
    let text = document.create_text(b"\xC3(\xED\xA0\x80\xEF\xBF\xBE\xF8\x90\x80\x80\xC3".to_vec());
    assert_eq!(
        string(&document, text, None, AS_XML),
        "&#xFFFD;(&#xFFFD;&#xFFFD;&#xFFFD;&#xFFFD;&#x10000;&#xFFFD;"
    );
    let element = document.create_element_named("p");
    document.set_attribute(element, "a", b"\xC3\xA9\xC3(\xC3".to_vec());
    // a last byte is written as it is
    let output = write(&document, element, None, b"", AS_XML).unwrap();
    assert_eq!(output.bytes, b"<p a=\"&#xE9;&#xFFFD;(\xC3\"/>");
}

#[test]
fn a_documents_encoding_is_switched_to_only_with_a_declaration() {
    let mut document = parse("<p>\u{e9}</p>");
    document.set_encoding(Some(b"ISO-8859-1".to_vec()));
    let output = write(&document, NodeId::DOCUMENT, None, b"", AS_XML).unwrap();
    assert_eq!(output.encoding.as_deref(), Some(&b"ISO-8859-1"[..]));
    assert!(output
        .bytes
        .ends_with("<p>\u{e9}</p></body></html>\n".as_bytes()));
    let output = write(
        &document,
        NodeId::DOCUMENT,
        None,
        b"",
        AS_XML | NO_DECLARATION,
    )
    .unwrap();
    assert_eq!(output.encoding, None);
    assert!(output.bytes.ends_with(b"<p>&#xE9;</p></body></html>\n"));
    document.set_encoding(Some(b"utf8".to_vec()));
    let output = write(&document, NodeId::DOCUMENT, None, b"", AS_XML).unwrap();
    assert_eq!(output.encoding, None);
}

#[test]
fn indentation_stops_at_sixty_bytes() {
    let document = parse(&"<div>".repeat(40));
    let output = string(
        &document,
        NodeId::DOCUMENT,
        None,
        AS_XML | FORMAT | NO_DECLARATION,
    );
    let deepest = output
        .lines()
        .find(|line| line.ends_with("<div/>"))
        .unwrap();
    assert_eq!(deepest, format!("{}<div/>", " ".repeat(60)));
    let long = "x".repeat(61);
    let options = AS_XML | FORMAT | NO_DECLARATION;
    let output = write(&document, NodeId::DOCUMENT, None, long.as_bytes(), options).unwrap();
    assert!(!String::from_utf8(output.bytes).unwrap().contains('x'));
}

#[test]
fn doctypes_quote_as_libxml2_does() {
    let mut document = Document::new(DocumentKind::Xml);
    let doctype = document.create_doctype(Doctype {
        name: b"html".to_vec(),
        public_id: Some(b"a\"b".to_vec()),
        system_id: None,
    });
    assert_eq!(
        string(&document, doctype, None, AS_XML),
        "<!DOCTYPE html PUBLIC 'a\"b' \"\">"
    );
    let doctype = document.create_doctype(Doctype {
        name: b"html".to_vec(),
        public_id: None,
        system_id: Some(b"'\"".to_vec()),
    });
    assert_eq!(
        string(&document, doctype, None, AS_XML),
        "<!DOCTYPE html SYSTEM \"'&quot;\">"
    );
}

#[test]
fn an_xhtml_doctype_gives_xhtml_rules_to_xml_output() {
    let document =
        parse("<!DOCTYPE html PUBLIC \"-//W3C//DTD XHTML 1.0 Strict//EN\" \"\"><p>x<br>");
    let output = string(&document, NodeId::DOCUMENT, None, AS_XML | NO_DECLARATION);
    assert!(output.contains("<p>x<br /></p>"), "{output}");
    let output = string(
        &document,
        NodeId::DOCUMENT,
        None,
        AS_XML | NO_DECLARATION | NO_XHTML,
    );
    assert!(output.contains("<p>x<br/></p>"), "{output}");
}

#[test]
fn deep_trees_cost_no_stack() {
    let depth = if cfg!(miri) { 100 } else { 5_000 };
    let serialize = move || {
        let input = "<div>".repeat(depth);
        let (document, _) = parse_document(input.as_bytes(), &Options::default()).unwrap();
        let xml = write(&document, NodeId::DOCUMENT, None, b"", AS_XML).unwrap();
        let xhtml = write(&document, NodeId::DOCUMENT, None, b"", AS_XHTML).unwrap();
        (xml.bytes.len(), xhtml.bytes.len())
    };
    let small_stack = std::thread::Builder::new().stack_size(256 * 1024);
    let lengths = small_stack.spawn(serialize).unwrap().join().unwrap();
    assert!(lengths.0 > 11 * depth && lengths.1 > 11 * depth);
}
