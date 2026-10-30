//! Nokogiri's `html_standard_serialize` (`ext/nokogiri/xml_node.c`), which
//! writes the HTML standard's serialization of a node, the node included.

use super::{is_html, VOID_ELEMENTS};
use crate::dom::{c_string, Attribute, Document, NodeId, NodeKind, QualName};
use html5ever::ns;

/// Elements whose text is written as is.
const RAW_TEXT_ELEMENTS: &[&str] = &[
    "style",
    "script",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "plaintext",
    "noscript",
];

/// The node's HTML. With `preserve_newline`, a newline that starts a `pre`,
/// `textarea` or `listing` is written twice, so that parsing the output
/// keeps it. Walks the tree without recursion, so depth costs no stack.
pub fn serialize(document: &Document, id: NodeId, preserve_newline: bool) -> Vec<u8> {
    enum Step {
        Open(NodeId),
        Close(NodeId),
    }

    let mut out = Vec::new();
    let mut steps = vec![Step::Open(id)];
    while let Some(step) = steps.pop() {
        let id = match step {
            Step::Open(id) => id,
            Step::Close(id) => {
                if let NodeKind::Element(element) = document.node_kind(id) {
                    out.extend_from_slice(b"</");
                    write_tag_name(&mut out, &element.name);
                    out.push(b'>');
                }
                continue;
            }
        };
        let open_children = match document.node_kind(id) {
            NodeKind::Element(element) => {
                out.push(b'<');
                write_tag_name(&mut out, &element.name);
                for &attribute in &element.attributes {
                    if let NodeKind::Attribute(attribute) = document.node_kind(attribute) {
                        out.push(b' ');
                        write_attribute(&mut out, attribute);
                    }
                }
                out.push(b'>');
                if is_html(&element.name) && VOID_ELEMENTS.contains(&&*element.name.local) {
                    false
                } else {
                    if preserve_newline && prepend_newline(document, id) {
                        out.push(b'\n');
                    }
                    steps.push(Step::Close(id));
                    true
                }
            }
            NodeKind::Attribute(attribute) => {
                write_attribute(&mut out, attribute);
                false
            }
            NodeKind::Text(text) => {
                let raw = document.parent(id).is_some_and(|parent| {
                    matches!(document.node_kind(parent), NodeKind::Element(e)
                        if is_html(&e.name) && RAW_TEXT_ELEMENTS.contains(&&*e.name.local))
                });
                if raw {
                    out.extend_from_slice(c_string(text));
                } else {
                    write_escaped(&mut out, text, false);
                }
                false
            }
            NodeKind::CData(text) => {
                write_all(&mut out, &[b"<![CDATA[", c_string(text), b"]]>"]);
                false
            }
            NodeKind::Comment(text) => {
                write_all(&mut out, &[b"<!--", c_string(text), b"-->"]);
                false
            }
            // libxml2 keeps a processing instruction's data as its content
            NodeKind::ProcessingInstruction(pi) => {
                write_all(&mut out, &[b"<?", c_string(&pi.data), b">"]);
                false
            }
            NodeKind::Doctype(doctype) => {
                write_all(&mut out, &[b"<!DOCTYPE ", c_string(&doctype.name), b">"]);
                false
            }
            NodeKind::Document | NodeKind::DocumentFragment => true,
        };
        if open_children {
            let first = steps.len();
            steps.extend(document.children(id).map(Step::Open));
            steps[first..].reverse();
        }
    }
    out
}

/// Whether a `pre`, `textarea` or `listing` starts with a newline, which
/// Nokogiri's `prepend_newline?` reports. It checks the name only.
pub fn prepend_newline(document: &Document, id: NodeId) -> bool {
    let NodeKind::Element(element) = document.node_kind(id) else {
        return false;
    };
    matches!(&*element.name.local, "pre" | "textarea" | "listing")
        && document.first_child(id).is_some_and(|child| {
            matches!(document.node_kind(child), NodeKind::Text(text) if text.first() == Some(&b'\n'))
        })
}

/// HTML, SVG and MathML elements are written without a prefix.
fn write_tag_name(out: &mut Vec<u8>, name: &QualName) {
    let foreign = ![ns!(), ns!(html), ns!(svg), ns!(mathml)].contains(&name.ns);
    if let (true, Some(prefix)) = (foreign, &name.prefix) {
        write_all(out, &[prefix.as_bytes(), b":"]);
    }
    out.extend_from_slice(name.local.as_bytes());
}

fn write_attribute(out: &mut Vec<u8>, attribute: &Attribute) {
    let name = &attribute.name;
    let prefix = match &name.ns {
        ns if *ns == ns!(xml) => Some("xml"),
        ns if *ns == ns!(xmlns) && &*name.local != "xmlns" => Some("xmlns"),
        ns if *ns == ns!(xlink) => Some("xlink"),
        ns if *ns == ns!() || *ns == ns!(xmlns) => None,
        _ => name.prefix.as_deref(),
    };
    if let Some(prefix) = prefix {
        write_all(out, &[prefix.as_bytes(), b":"]);
    }
    out.extend_from_slice(name.local.as_bytes());
    out.extend_from_slice(b"=\"");
    write_escaped(out, &attribute.value, true);
    out.push(b'"');
}

fn write_escaped(out: &mut Vec<u8>, text: &[u8], attribute: bool) {
    let text = c_string(text);
    let mut start = 0;
    let mut index = 0;
    while index < text.len() {
        let (replacement, length): (&[u8], usize) = match text[index] {
            b'&' => (b"&amp;", 1),
            0xC2 if text.get(index + 1) == Some(&0xA0) => (b"&nbsp;", 2),
            b'"' if attribute => (b"&quot;", 1),
            b'<' if !attribute => (b"&lt;", 1),
            b'>' if !attribute => (b"&gt;", 1),
            _ => {
                index += 1;
                continue;
            }
        };
        write_all(out, &[&text[start..index], replacement]);
        index += length;
        start = index;
    }
    out.extend_from_slice(&text[start..]);
}

fn write_all(out: &mut Vec<u8>, parts: &[&[u8]]) {
    for part in parts {
        out.extend_from_slice(part);
    }
}
