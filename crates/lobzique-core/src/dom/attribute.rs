//! Finding attributes by name, as the libxml2 functions behind Nokogiri's
//! attribute readers do. Names are compared as bytes, since Ruby hands them
//! over in any encoding.

use super::{Attribute, Document, NodeId, NodeKind};
use html5ever::ns;
use std::iter;

impl Document {
    /// The first attribute with this local name, in any namespace:
    /// libxml2's `xmlHasProp`.
    pub fn attribute_named(&self, element: NodeId, name: &[u8]) -> Option<NodeId> {
        self.find_attribute(element, |attribute| attribute.name.local.as_bytes() == name)
    }

    /// The attribute with this local name in the namespace `href`, or in no
    /// namespace if `href` is `None`: libxml2's `xmlHasNsProp`.
    pub fn attribute_in_namespace(
        &self,
        element: NodeId,
        name: &[u8],
        href: Option<&[u8]>,
    ) -> Option<NodeId> {
        self.find_attribute(element, |attribute| {
            let namespace = &attribute.name.ns;
            attribute.name.local.as_bytes() == name
                && match href {
                    Some(href) => *namespace != ns!() && namespace.as_bytes() == href,
                    None => *namespace == ns!(),
                }
        })
    }

    /// Nokogiri's `Node#[]`. A name with a colon is `prefix:local` when the
    /// prefix is in scope, and otherwise matches an attribute with that
    /// whole name in any namespace; a name without one matches attributes in
    /// no namespace only.
    pub fn attribute_value(&self, element: NodeId, name: &[u8]) -> Option<&[u8]> {
        let attribute = match name.iter().position(|&byte| byte == b':') {
            Some(colon) => match self.search_namespace(element, &name[..colon]) {
                Some(namespace) => self.attribute_in_namespace(
                    element,
                    &name[colon + 1..],
                    Some(namespace.href.as_bytes()),
                ),
                None => self.attribute_named(element, name),
            },
            None => self.attribute_in_namespace(element, name, None),
        }?;
        self.value_of(attribute)
    }

    /// The nearest `xml:lang` on the node or an ancestor: libxml2's
    /// `xmlNodeGetLang`. A `lang` attribute doesn't count.
    pub fn lang(&self, id: NodeId) -> Option<&[u8]> {
        let href = ns!(xml);
        iter::once(id)
            .chain(self.ancestors(id))
            .find_map(|node| self.attribute_in_namespace(node, b"lang", Some(href.as_bytes())))
            .and_then(|attribute| self.value_of(attribute))
    }

    fn value_of(&self, attribute: NodeId) -> Option<&[u8]> {
        match self.node_kind(attribute) {
            NodeKind::Attribute(attribute) => Some(&attribute.value),
            _ => None,
        }
    }

    fn find_attribute(
        &self,
        element: NodeId,
        matches: impl Fn(&Attribute) -> bool,
    ) -> Option<NodeId> {
        self.attributes(element).iter().copied().find(|&id| {
            matches!(self.node_kind(id), NodeKind::Attribute(attribute) if matches(attribute))
        })
    }
}
