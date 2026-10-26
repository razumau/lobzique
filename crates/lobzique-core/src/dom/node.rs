use super::{NodeId, QualName};
use std::mem::size_of;

/// Node types numbered as libxml2 numbers them, which is what Nokogiri's
/// `Node#type` and `Node::*_NODE` constants expose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum NodeType {
    Element = 1,
    Attribute = 2,
    Text = 3,
    CDataSection = 4,
    ProcessingInstruction = 7,
    Comment = 8,
    Document = 9,
    DocumentFragment = 11,
    HtmlDocument = 13,
    Dtd = 14,
}

#[derive(Clone, Debug)]
pub enum NodeKind {
    Document,
    DocumentFragment,
    Element(Element),
    Attribute(Attribute),
    Text(Vec<u8>),
    CData(Vec<u8>),
    Comment(Vec<u8>),
    Doctype(Doctype),
    ProcessingInstruction(ProcessingInstruction),
}

impl NodeKind {
    /// The type of every kind but `Document`, whose type depends on the
    /// document's [`super::DocumentKind`].
    pub(super) fn node_type(&self) -> NodeType {
        match self {
            NodeKind::Document => NodeType::Document,
            NodeKind::DocumentFragment => NodeType::DocumentFragment,
            NodeKind::Element(_) => NodeType::Element,
            NodeKind::Attribute(_) => NodeType::Attribute,
            NodeKind::Text(_) => NodeType::Text,
            NodeKind::CData(_) => NodeType::CDataSection,
            NodeKind::Comment(_) => NodeType::Comment,
            NodeKind::Doctype(_) => NodeType::Dtd,
            NodeKind::ProcessingInstruction(_) => NodeType::ProcessingInstruction,
        }
    }

    /// Interned names belong to html5ever's atom table, not to the node.
    pub(super) fn heap_size(&self) -> usize {
        match self {
            NodeKind::Document | NodeKind::DocumentFragment => 0,
            NodeKind::Element(element) => element.attributes.capacity() * size_of::<NodeId>(),
            NodeKind::Attribute(attribute) => attribute.value.capacity(),
            NodeKind::Text(text) | NodeKind::CData(text) | NodeKind::Comment(text) => {
                text.capacity()
            }
            NodeKind::Doctype(doctype) => {
                doctype.name.capacity()
                    + doctype.public_id.as_ref().map_or(0, Vec::capacity)
                    + doctype.system_id.as_ref().map_or(0, Vec::capacity)
            }
            NodeKind::ProcessingInstruction(pi) => pi.target.capacity() + pi.data.capacity(),
        }
    }

    pub(super) fn can_have_children(&self) -> bool {
        matches!(
            self,
            NodeKind::Document | NodeKind::DocumentFragment | NodeKind::Element(_)
        )
    }
}

#[derive(Clone, Debug)]
pub struct Element {
    pub name: QualName,
    /// Attribute nodes in source order. Each one's parent is this element,
    /// but it is not among the element's children.
    pub attributes: Vec<NodeId>,
}

#[derive(Clone, Debug)]
pub struct Attribute {
    pub name: QualName,
    pub value: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Doctype {
    pub name: Vec<u8>,
    pub public_id: Option<Vec<u8>>,
    pub system_id: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct ProcessingInstruction {
    pub target: Vec<u8>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub(super) struct Node {
    pub(super) kind: NodeKind,
    pub(super) parent: Option<NodeId>,
    pub(super) previous_sibling: Option<NodeId>,
    pub(super) next_sibling: Option<NodeId>,
    pub(super) first_child: Option<NodeId>,
    pub(super) last_child: Option<NodeId>,
}

impl Node {
    pub(super) fn new(kind: NodeKind) -> Node {
        Node {
            kind,
            parent: None,
            previous_sibling: None,
            next_sibling: None,
            first_child: None,
            last_child: None,
        }
    }
}
