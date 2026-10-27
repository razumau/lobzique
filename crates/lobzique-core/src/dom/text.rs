use super::{Document, NodeId, NodeKind};

impl Document {
    /// libxml2's `xmlNodeGetContent`, which Nokogiri's `Node#content`
    /// returns: the text and CDATA in an element, fragment or document, a
    /// character node's own text, and nothing for a doctype.
    pub fn content(&self, id: NodeId) -> Option<Vec<u8>> {
        match self.node_kind(id) {
            NodeKind::Text(text) | NodeKind::CData(text) | NodeKind::Comment(text) => {
                Some(text.clone())
            }
            NodeKind::Attribute(attribute) => Some(attribute.value.clone()),
            NodeKind::ProcessingInstruction(pi) => Some(pi.data.clone()),
            NodeKind::Doctype(_) => None,
            NodeKind::Document | NodeKind::DocumentFragment | NodeKind::Element(_) => {
                let mut content = Vec::new();
                for descendant in self.descendants(id) {
                    if let NodeKind::Text(text) | NodeKind::CData(text) = self.node_kind(descendant)
                    {
                        content.extend_from_slice(text);
                    }
                }
                Some(content)
            }
        }
    }

    /// The bytes of a text, CDATA or comment node, to change in place.
    pub fn character_data_mut(&mut self, id: NodeId) -> Option<&mut Vec<u8>> {
        match &mut self.node_mut(id).kind {
            NodeKind::Text(text) | NodeKind::CData(text) | NodeKind::Comment(text) => Some(text),
            _ => None,
        }
    }

    /// Turns a text node into a CDATA node or back; other nodes stay as
    /// they are.
    pub fn set_cdata(&mut self, id: NodeId, cdata: bool) {
        let kind = &mut self.node_mut(id).kind;
        *kind = match std::mem::replace(kind, NodeKind::Document) {
            NodeKind::Text(text) | NodeKind::CData(text) if cdata => NodeKind::CData(text),
            NodeKind::Text(text) | NodeKind::CData(text) => NodeKind::Text(text),
            other => other,
        };
    }
}
