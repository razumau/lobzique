//! The arena DOM. A [`Document`] owns every node created for it, and frees
//! none of them before it is dropped itself: unlinked nodes stay valid, the
//! way they do in a libxml2 document that Nokogiri keeps them pinned to.
//!
//! Element and attribute names follow the HTML standard, so HTML elements are
//! in the HTML namespace; the bindings show that namespace as `nil`, as
//! Nokogiri does. Names are UTF-8 atoms shared with the parser and selector
//! engine. Everything else (text, attribute values, doctype fields) is bytes
//! kept exactly as given, as libxml2 keeps what Nokogiri passes it.

mod attribute;
mod check;
mod namespace;
mod node;
mod path;
#[cfg(test)]
mod tests;
mod text;
mod tree;

pub use html5ever::{LocalName, Namespace, Prefix, QualName};
pub use namespace::{NamespaceDefinition, NamespaceRef};
pub use node::{Attribute, Doctype, Element, NodeKind, NodeType, ProcessingInstruction};
pub use text::encode_special_chars;
pub use tree::{Ancestors, Children, Descendants, HierarchyError};

use node::Node;
use std::mem::size_of;
use std::num::NonZeroU32;

/// A node's index in its document's arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(NonZeroU32);

impl NodeId {
    /// The document node, the first one in every arena.
    pub const DOCUMENT: NodeId = NodeId(NonZeroU32::MIN);

    /// Zero-based position in the arena, for side tables indexed by node.
    pub fn index(self) -> usize {
        self.0.get() as usize - 1
    }

    fn from_index(index: usize) -> NodeId {
        u32::try_from(index + 1)
            .ok()
            .and_then(NonZeroU32::new)
            .map(NodeId)
            .expect("a document holds fewer than 2^32 nodes")
    }
}

/// Whether a document came from the HTML or the XML side, which decides its
/// node type, as libxml2's `htmlDocPtr` and `xmlDocPtr` do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    Html,
    Xml,
}

#[derive(Clone, Debug)]
pub struct Document {
    kind: DocumentKind,
    nodes: Vec<Node>,
    /// libxml2's `doc->encoding`: a label, kept as given.
    encoding: Option<Vec<u8>>,
}

impl Document {
    pub fn new(kind: DocumentKind) -> Document {
        Document {
            kind,
            nodes: vec![Node::new(NodeKind::Document)],
            encoding: None,
        }
    }

    /// An HTML document with the doctype libxml2's `htmlNewDoc` gives it,
    /// which `HTML4::Document.new` and so `HTML5::Document.new` start with.
    pub fn new_html4(system_id: Option<Vec<u8>>, public_id: Option<Vec<u8>>) -> Document {
        let mut document = Document::new(DocumentKind::Html);
        let doctype = if system_id.is_none() && public_id.is_none() {
            Doctype {
                name: "html".into(),
                public_id: Some("-//W3C//DTD HTML 4.0 Transitional//EN".into()),
                system_id: Some("http://www.w3.org/TR/REC-html40/loose.dtd".into()),
            }
        } else {
            Doctype {
                name: "html".into(),
                public_id,
                system_id,
            }
        };
        let doctype = document.create_doctype(doctype);
        document
            .append_child(NodeId::DOCUMENT, doctype)
            .expect("a new document takes a doctype");
        document
    }

    pub fn kind(&self) -> DocumentKind {
        self.kind
    }

    pub fn encoding(&self) -> Option<&[u8]> {
        self.encoding.as_deref()
    }

    pub fn set_encoding(&mut self, encoding: Option<Vec<u8>>) {
        self.encoding = encoding;
    }

    /// Every node in the arena, linked or not, in creation order.
    pub fn node_ids(&self) -> impl ExactSizeIterator<Item = NodeId> {
        (0..self.nodes.len()).map(NodeId::from_index)
    }

    pub fn node_kind(&self, id: NodeId) -> &NodeKind {
        &self.node(id).kind
    }

    pub fn node_type(&self, id: NodeId) -> NodeType {
        match self.node_kind(id) {
            NodeKind::Document => match self.kind {
                DocumentKind::Html => NodeType::HtmlDocument,
                DocumentKind::Xml => NodeType::Document,
            },
            kind => kind.node_type(),
        }
    }

    /// libxml2's `node->name`: what Nokogiri's native `node_name` returns.
    pub fn node_name(&self, id: NodeId) -> Option<&[u8]> {
        match self.node_kind(id) {
            NodeKind::Element(element) => Some(element.name.local.as_bytes()),
            NodeKind::Attribute(attribute) => Some(attribute.name.local.as_bytes()),
            NodeKind::Text(_) => Some(b"text"),
            NodeKind::Comment(_) => Some(b"comment"),
            NodeKind::Doctype(doctype) => Some(&doctype.name),
            NodeKind::ProcessingInstruction(pi) => Some(&pi.target),
            NodeKind::Document | NodeKind::DocumentFragment | NodeKind::CData(_) => None,
        }
    }

    pub fn create_element(&mut self, name: QualName) -> NodeId {
        self.create(NodeKind::Element(Element {
            name,
            attributes: Vec::new(),
            namespace_definitions: Vec::new(),
        }))
    }

    /// An element as `Node.new` makes one: the name taken as is, in the HTML
    /// namespace in HTML documents and in none in XML ones.
    pub fn create_element_named(&mut self, name: &str) -> NodeId {
        let namespace = match self.kind {
            DocumentKind::Html => html5ever::ns!(html),
            DocumentKind::Xml => html5ever::ns!(),
        };
        self.create_element(QualName::new(None, namespace, LocalName::from(name)))
    }

    /// The document's doctype, libxml2's internal subset.
    pub fn doctype(&self) -> Option<NodeId> {
        self.children(NodeId::DOCUMENT)
            .find(|&child| matches!(self.node_kind(child), NodeKind::Doctype(_)))
    }

    pub fn create_attribute(&mut self, name: QualName, value: Vec<u8>) -> NodeId {
        self.create(NodeKind::Attribute(Attribute { name, value }))
    }

    pub fn create_text(&mut self, text: Vec<u8>) -> NodeId {
        self.create(NodeKind::Text(text))
    }

    pub fn create_cdata(&mut self, text: Vec<u8>) -> NodeId {
        self.create(NodeKind::CData(text))
    }

    pub fn create_comment(&mut self, text: Vec<u8>) -> NodeId {
        self.create(NodeKind::Comment(text))
    }

    pub fn create_fragment(&mut self) -> NodeId {
        self.create(NodeKind::DocumentFragment)
    }

    pub fn create_doctype(&mut self, doctype: Doctype) -> NodeId {
        self.create(NodeKind::Doctype(doctype))
    }

    pub fn create_processing_instruction(&mut self, target: Vec<u8>, data: Vec<u8>) -> NodeId {
        self.create(NodeKind::ProcessingInstruction(ProcessingInstruction {
            target,
            data,
        }))
    }

    /// Bytes the arena holds on the heap, for `ObjectSpace.memsize_of`.
    /// Walks every node.
    pub fn heap_size(&self) -> usize {
        self.nodes.capacity() * size_of::<Node>()
            + self.encoding.as_ref().map_or(0, Vec::capacity)
            + self
                .nodes
                .iter()
                .map(|node| node.kind.heap_size())
                .sum::<usize>()
    }

    fn create(&mut self, kind: NodeKind) -> NodeId {
        let id = NodeId::from_index(self.nodes.len());
        self.nodes.push(Node::new(kind));
        id
    }

    fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.index()]
    }
}
