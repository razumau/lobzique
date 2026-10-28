//! Finding attributes by name, as the libxml2 functions behind Nokogiri's
//! attribute readers do. Names are compared as bytes, since Ruby hands them
//! over in any encoding.

use super::namespace::split_qualified_name;
use super::{Attribute, Document, NodeId, NodeKind, QualName};
use html5ever::{ns, LocalName, Namespace, Prefix};
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

    /// Nokogiri's `Node#[]=`, libxml2's `xmlSetProp`: a name with a colon
    /// is `prefix:local` when the prefix is in scope and neither side is
    /// empty, and otherwise the whole name of an attribute in no namespace.
    /// An existing attribute keeps its node and gets the new value. `None`
    /// for anything but an element.
    pub fn set_attribute(&mut self, element: NodeId, name: &str, value: Vec<u8>) -> Option<NodeId> {
        if !matches!(self.node_kind(element), NodeKind::Element(_)) {
            return None;
        }
        let namespace = split_qualified_name(name).and_then(|(prefix, local)| {
            let namespace = self.search_namespace(element, prefix.as_bytes())?;
            Some((prefix, local, Namespace::from(namespace.href)))
        });
        let name = match namespace {
            Some((prefix, local, href)) => {
                QualName::new(Some(Prefix::from(prefix)), href, LocalName::from(local))
            }
            None => QualName::new(None, ns!(), LocalName::from(name)),
        };
        Some(self.set_attribute_name(element, name, value))
    }

    /// `Attr#value=`. Nokogiri escapes the value and libxml2 unescapes it
    /// again, so it is stored as given.
    pub fn set_attribute_value(&mut self, attribute: NodeId, value: Vec<u8>) {
        if let NodeKind::Attribute(attribute) = &mut self.node_mut(attribute).kind {
            attribute.value = value;
        }
    }

    /// libxml2's `xmlNodeSetLang`: sets `xml:lang` on an element.
    pub fn set_lang(&mut self, id: NodeId, lang: Vec<u8>) {
        if matches!(self.node_kind(id), NodeKind::Element(_)) {
            let name = QualName::new(Some(Prefix::from("xml")), ns!(xml), LocalName::from("lang"));
            self.set_attribute_name(id, name, lang);
        }
    }

    /// libxml2's `xmlSetNsProp`.
    fn set_attribute_name(&mut self, element: NodeId, name: QualName, value: Vec<u8>) -> NodeId {
        let href = (name.ns != ns!()).then(|| name.ns.clone());
        let existing = self.attribute_in_namespace(
            element,
            name.local.as_bytes(),
            href.as_ref().map(|href| href.as_bytes()),
        );
        match existing {
            Some(attribute) => {
                if let NodeKind::Attribute(existing) = &mut self.node_mut(attribute).kind {
                    existing.name = name;
                    existing.value = value;
                }
                attribute
            }
            None => {
                let attribute = self.create_attribute(name, value);
                self.append_attribute(element, attribute)
                    .expect("checked to be an element");
                attribute
            }
        }
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
