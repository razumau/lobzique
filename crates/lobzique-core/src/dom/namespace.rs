//! Namespaces as libxml2 records them for Nokogiri: an element or attribute
//! is in at most one, and elements carry the definitions that bring
//! prefixes into scope (libxml2's `nsDef`).

use super::{Document, NodeId, NodeKind};
use html5ever::{ns, Namespace, Prefix};
use std::iter;

/// A namespace as libxml2 records it on a node: Nokogiri's `XML::Namespace`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NamespaceRef<'a> {
    pub prefix: Option<&'a str>,
    pub href: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamespaceDefinition {
    pub prefix: Option<Prefix>,
    pub href: Namespace,
}

impl<'a> From<&'a NamespaceDefinition> for NamespaceRef<'a> {
    fn from(definition: &'a NamespaceDefinition) -> NamespaceRef<'a> {
        NamespaceRef {
            prefix: definition.prefix.as_deref(),
            href: &definition.href,
        }
    }
}

/// libxml2 binds the `xml` prefix in every document, with no definition.
const XML: NamespaceRef<'static> = NamespaceRef {
    prefix: Some("xml"),
    href: "http://www.w3.org/XML/1998/namespace",
};

impl Document {
    /// The node's namespace as Nokogiri shows it after an HTML5 parse: none
    /// for HTML elements, and the prefix gumbo gives each foreign namespace.
    pub fn namespace(&self, id: NodeId) -> Option<NamespaceRef<'_>> {
        let (name, prefixes): (_, &[(Namespace, &str)]) = match self.node_kind(id) {
            NodeKind::Element(element) => {
                (&element.name, &[(ns!(svg), "svg"), (ns!(mathml), "math")])
            }
            NodeKind::Attribute(attribute) => (
                &attribute.name,
                &[
                    (ns!(xlink), "xlink"),
                    (ns!(xml), "xml"),
                    (ns!(xmlns), "xmlns"),
                ],
            ),
            _ => return None,
        };
        if name.ns == ns!() || name.ns == ns!(html) {
            return None;
        }
        let prefix = prefixes
            .iter()
            .find(|(namespace, _)| *namespace == name.ns)
            .map(|&(_, prefix)| prefix)
            .or(name.prefix.as_deref());
        Some(NamespaceRef {
            prefix,
            href: &name.ns,
        })
    }

    /// The namespaces defined on an element, in the order they were added.
    pub fn namespace_definitions(&self, id: NodeId) -> &[NamespaceDefinition] {
        match self.node_kind(id) {
            NodeKind::Element(element) => &element.namespace_definitions,
            _ => &[],
        }
    }

    /// Defines a namespace on an element, unless its prefix already has a
    /// definition there, as libxml2's `xmlNewNs` does. Other nodes take none.
    pub fn define_namespace(&mut self, element: NodeId, prefix: Option<Prefix>, href: Namespace) {
        if let NodeKind::Element(element) = &mut self.node_mut(element).kind {
            let definitions = &mut element.namespace_definitions;
            if definitions.iter().all(|defined| defined.prefix != prefix) {
                definitions.push(NamespaceDefinition { prefix, href });
            }
        }
    }

    /// Every namespace in scope at the node, nearest definition first, one
    /// per prefix: libxml2's `xmlGetNsList`.
    pub fn namespace_scopes(&self, id: NodeId) -> Vec<NamespaceRef<'_>> {
        let mut scopes: Vec<NamespaceRef<'_>> = Vec::new();
        for node in iter::once(id).chain(self.ancestors(id)) {
            for definition in self.namespace_definitions(node) {
                let definition = NamespaceRef::from(definition);
                if scopes.iter().all(|scope| scope.prefix != definition.prefix) {
                    scopes.push(definition);
                }
            }
        }
        scopes
    }

    /// The namespace `prefix` refers to at the node: libxml2's
    /// `xmlSearchNs`, which also accepts the namespace of an ancestor
    /// element that has no definition in scope.
    pub fn search_namespace(&self, id: NodeId, prefix: &[u8]) -> Option<NamespaceRef<'_>> {
        if prefix == b"xml" {
            return Some(XML);
        }
        let has_prefix =
            |namespace: &NamespaceRef<'_>| namespace.prefix.map(str::as_bytes) == Some(prefix);
        for node in iter::once(id).chain(self.ancestors(id)) {
            let defined = self
                .namespace_definitions(node)
                .iter()
                .map(NamespaceRef::from)
                .find(has_prefix);
            if defined.is_some() {
                return defined;
            }
            if node != id && matches!(self.node_kind(node), NodeKind::Element(_)) {
                if let Some(namespace) = self.namespace(node).filter(has_prefix) {
                    return Some(namespace);
                }
            }
        }
        None
    }
}
