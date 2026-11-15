//! Copies nodes as libxml2's `xmlDocCopyNode` and `xmlCopyDoc` do. Used
//! for Nokogiri's `dup`, for nodes that move between documents, and for nodes
//! that are reparented as text.

use super::{Document, DocumentKind, NodeId, NodeKind, QualName};
use html5ever::{ns, LocalName, Namespace, Prefix};
use std::iter;

/// How much of a node a copy takes: Nokogiri's `dup(level)`. libxml2 reads
/// the level as 0, 2, or anything else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyDepth {
    /// The node alone. An element loses its attributes, namespace
    /// definitions and namespace.
    Shallow,
    /// The node with its attributes and namespace definitions.
    Attributes,
    Deep,
}

impl CopyDepth {
    pub fn from_level(level: i32) -> CopyDepth {
        match level {
            0 => CopyDepth::Shallow,
            2 => CopyDepth::Attributes,
            _ => CopyDepth::Deep,
        }
    }
}

impl Document {
    /// A detached copy of a node of this document. Documents and doctypes
    /// are not copied as nodes, so there is no copy for them.
    pub fn copy(&mut self, id: NodeId, depth: CopyDepth) -> Option<NodeId> {
        self.copy_from(None, id, depth)
    }

    pub fn import(&mut self, source: &Document, id: NodeId, depth: CopyDepth) -> Option<NodeId> {
        self.copy_from(Some(source), id, depth)
    }

    /// `xmlCopyDoc`: the encoding and version. With `deep`, the whole tree.
    /// Unlinked nodes stay behind.
    pub fn copy_document(&self, deep: bool) -> Document {
        let mut copy = Document::new(self.kind);
        copy.encoding.clone_from(&self.encoding);
        copy.version.clone_from(&self.version);
        if !deep {
            return copy;
        }
        for child in self.children(NodeId::DOCUMENT) {
            let child = match self.node_kind(child) {
                NodeKind::Doctype(doctype) => Some(copy.create_doctype(doctype.clone())),
                _ => copy.import(self, child, CopyDepth::Deep),
            };
            if let Some(child) = child {
                copy.append_child(NodeId::DOCUMENT, child)
                    .expect("a document takes what another document had");
            }
        }
        copy
    }

    /// Walks the source subtree by its links, as `descendants` does. Depth then
    /// uses no stack, and no borrow of a same-document source is held
    /// while copies are made.
    fn copy_from(
        &mut self,
        source: Option<&Document>,
        id: NodeId,
        depth: CopyDepth,
    ) -> Option<NodeId> {
        let root = self.copy_node(source, id, depth, None)?;
        if depth != CopyDepth::Deep {
            return Some(root);
        }
        // Each source node whose children are being copied, with its copy.
        let mut open = vec![(id, root)];
        let mut next = source.unwrap_or(self).first_child(id);
        while let Some(node) = next {
            let &(_, parent) = open.last().expect("the root stays open until the end");
            let copy = self.copy_node(source, node, depth, Some((parent, root)));
            let first_child = source.unwrap_or(self).first_child(node);
            if let (Some(copy), Some(first_child)) = (copy, first_child) {
                open.push((node, copy));
                next = Some(first_child);
                continue;
            }
            let mut finished = node;
            next = loop {
                if let Some(sibling) = source.unwrap_or(self).next_sibling(finished) {
                    break Some(sibling);
                }
                let (parent, _) = open.pop().expect("the root stays open until the end");
                if open.is_empty() {
                    break None;
                }
                finished = parent;
            };
        }
        Some(root)
    }

    /// Copies one node with its attributes and appends it to `parent`,
    /// the first of `placement`. The second is the root of the copy,
    /// where the namespaces that the copy needs are defined.
    fn copy_node(
        &mut self,
        source: Option<&Document>,
        id: NodeId,
        depth: CopyDepth,
        placement: Option<(NodeId, NodeId)>,
    ) -> Option<NodeId> {
        let src = source.unwrap_or(self);
        let mut attributes = Vec::new();
        let kind = match src.node_kind(id) {
            NodeKind::Document | NodeKind::Doctype(_) => return None,
            NodeKind::Element(element) if depth == CopyDepth::Shallow => {
                let mut copy = element.clone();
                copy.name = QualName::new(None, self.default_namespace(), copy.name.local);
                copy.attributes.clear();
                copy.namespace_definitions.clear();
                NodeKind::Element(copy)
            }
            NodeKind::Element(element) => {
                attributes = element
                    .attributes
                    .iter()
                    .filter_map(|&attribute| match src.node_kind(attribute) {
                        NodeKind::Attribute(attribute) => Some(attribute.clone()),
                        _ => None,
                    })
                    .collect();
                let mut copy = element.clone();
                copy.attributes.clear();
                copy.name = adopt_name(copy.name, src.kind, self.kind);
                NodeKind::Element(copy)
            }
            kind => kind.clone(),
        };
        let reconciles = matches!(kind, NodeKind::Element(_)) && depth != CopyDepth::Shallow
            || matches!(kind, NodeKind::Attribute(_));
        let copy = self.create(kind);
        let root = match placement {
            Some((parent, root)) => {
                self.append_child(parent, copy)
                    .expect("a copy goes where its original was");
                root
            }
            None => copy,
        };
        if reconciles {
            self.reconcile_namespace(copy, root);
        }
        for attribute in attributes {
            let attribute = self.create(NodeKind::Attribute(attribute));
            self.append_attribute(copy, attribute)
                .expect("attributes go on the element copy");
            self.reconcile_namespace(attribute, root);
        }
        Some(copy)
    }

    /// A copied element or attribute whose namespace prefix has no definition
    /// in the copy gets one on the root of the copy, as libxml2 defines
    /// it when it copies. Only an element root can take one.
    fn reconcile_namespace(&mut self, id: NodeId, root: NodeId) {
        let Some(namespace) = self.namespace(id) else {
            return;
        };
        let Some(prefix) = namespace.prefix.filter(|&prefix| prefix != "xml") else {
            return;
        };
        let element = match self.node_kind(id) {
            NodeKind::Attribute(_) => self.parent(id),
            _ => Some(id),
        };
        let defined = element.is_some_and(|element| {
            iter::once(element)
                .chain(self.ancestors(element))
                .flat_map(|node| self.namespace_definitions(node))
                .any(|definition| definition.prefix.as_deref() == Some(prefix))
        });
        if !defined {
            let (prefix, href) = (Prefix::from(prefix), Namespace::from(namespace.href));
            self.define_namespace(root, Some(prefix), href);
        }
    }

    /// The namespace of an element with none in libxml2's terms: HTML in HTML
    /// documents, as for `Node.new`.
    pub(super) fn default_namespace(&self) -> Namespace {
        no_namespace(self.kind)
    }

    pub(super) fn rename(&mut self, id: NodeId, name: QualName) {
        match &mut self.node_mut(id).kind {
            NodeKind::Element(element) => element.name = name,
            NodeKind::Attribute(attribute) => attribute.name = name,
            _ => {}
        }
    }

    pub(super) fn local_name(&self, id: NodeId) -> Option<&LocalName> {
        match self.node_kind(id) {
            NodeKind::Element(element) => Some(&element.name.local),
            NodeKind::Attribute(attribute) => Some(&attribute.name.local),
            _ => None,
        }
    }
}

/// What stands for no namespace, in libxml2's terms, in a kind of document.
fn no_namespace(kind: DocumentKind) -> Namespace {
    match kind {
        DocumentKind::Html => ns!(html),
        DocumentKind::Xml => ns!(),
    }
}

/// An element name from a document of kind `from`, as a document of kind
/// `to` writes it.
fn adopt_name(name: QualName, from: DocumentKind, to: DocumentKind) -> QualName {
    if name.prefix.is_none() && name.ns == no_namespace(from) {
        QualName::new(None, no_namespace(to), name.local)
    } else {
        name
    }
}
