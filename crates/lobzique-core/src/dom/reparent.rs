//! Nokogiri's `reparent_node_with` over libxml2 2.13: which nodes may go
//! where, the copies made of text and of nodes from other documents, text
//! merging, and the namespace fixes afterwards (`relink_namespace`).

use super::namespace::split_qualified_name;
use super::{CopyDepth, Document, NodeId, NodeKind, NodeType, QualName};
use html5ever::{LocalName, Namespace, Prefix};
use std::{fmt, mem};

/// Where a node goes relative to the pivot, one per native method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    /// `add_child_node`, through `xmlAddChild`.
    LastChild,
    /// `add_previous_sibling_node`, through `xmlAddPrevSibling`.
    Before,
    /// `add_next_sibling_node`, through `xmlAddNextSibling`.
    After,
    /// `replace_node`, through `xmlReplaceNode`.
    Replace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReparentError {
    /// Nokogiri's DOM check refuses this kind of node under this parent.
    NotAllowed,
    /// libxml2 refuses to put a node into or next to itself, or next to a
    /// node with no parent. The node has been unlinked, as in Nokogiri.
    Refused,
    /// The node would become its own ancestor. Nokogiri raises after
    /// libxml2 has linked the cycle; here nothing has changed.
    Cycle,
}

impl fmt::Display for ReparentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ReparentError::NotAllowed => "node can't go there",
            ReparentError::Refused => "could not reparent node",
            ReparentError::Cycle => "node would become its own ancestor",
        })
    }
}

impl std::error::Error for ReparentError {}

impl Document {
    /// Moves a node of this document relative to `pivot`. Returns what is
    /// now in the tree for it: the node, its copy if it is text, or the
    /// text node the copy merged into.
    pub fn reparent(
        &mut self,
        pivot: NodeId,
        position: Position,
        node: NodeId,
        namespace_inheritance: bool,
    ) -> Result<NodeId, ReparentError> {
        self.reparent_with(pivot, position, None, node, namespace_inheritance)
    }

    /// [`Document::reparent`] for a node of another document, which is
    /// copied into this one and unlinked from its own.
    pub fn reparent_from(
        &mut self,
        pivot: NodeId,
        position: Position,
        source: &mut Document,
        node: NodeId,
        namespace_inheritance: bool,
    ) -> Result<NodeId, ReparentError> {
        self.reparent_with(pivot, position, Some(source), node, namespace_inheritance)
    }

    fn reparent_with(
        &mut self,
        pivot: NodeId,
        position: Position,
        source: Option<&mut Document>,
        node: NodeId,
        namespace_inheritance: bool,
    ) -> Result<NodeId, ReparentError> {
        let parent = match position {
            Position::LastChild => Some(pivot),
            _ => self.parent(pivot),
        };
        let node_type = source.as_deref().unwrap_or(self).node_type(node);
        let is_self = source.is_none() && pivot == node;
        if let Some(parent) = parent {
            if !may_contain(self.node_type(parent), node_type) {
                return Err(ReparentError::NotAllowed);
            }
        }

        let is_text = node_type == NodeType::Text;
        let inserted = if source.is_some() || is_text {
            let copy = match source.as_deref() {
                Some(source) => self.import(source, node, CopyDepth::Deep),
                None => self.copy(node, CopyDepth::Deep),
            };
            copy.ok_or(ReparentError::Refused)?
        } else {
            if pivot == node {
                self.detach(node);
                return Err(ReparentError::Refused);
            }
            if parent
                .is_some_and(|parent| parent == node || self.ancestors(parent).any(|a| a == node))
            {
                return Err(ReparentError::Cycle);
            }
            node
        };
        match source {
            Some(source) => source.detach(node),
            None => self.detach(node),
        }
        if is_self && matches!(position, Position::Before | Position::After) {
            // Text next to itself: Nokogiri links the copy beside the
            // unlinked original, outside any tree, and siblings here need a
            // parent, so the copy stays detached
            return Ok(inserted);
        }
        if position != Position::LastChild && self.parent(pivot).is_none() {
            return Err(ReparentError::Refused);
        }

        let reparented = match position {
            Position::LastChild => self.append_merging_text(pivot, inserted),
            Position::Before => self.insert_before(pivot, inserted).map(|()| inserted),
            Position::After => self.insert_after(pivot, inserted).map(|()| inserted),
            Position::Replace => self.replace_merging_text(pivot, inserted),
        }
        .map_err(|_| ReparentError::Refused)?;
        self.relink_namespaces(reparented, namespace_inheritance);
        Ok(reparented)
    }

    /// `xmlAddChild`: text after a text node is added to it instead.
    fn append_merging_text(
        &mut self,
        parent: NodeId,
        node: NodeId,
    ) -> Result<NodeId, super::HierarchyError> {
        if let Some(last) = self.last_child(parent) {
            if self.merge_text(last, node) {
                return Ok(last);
            }
        }
        self.append_child(parent, node)?;
        Ok(node)
    }

    /// `xmlReplaceNode`, then Nokogiri's merging of a text replacement
    /// with the text on either side. The text after it is first replaced
    /// by a copy, so the original, which may have an object, isn't merged
    /// away.
    fn replace_merging_text(
        &mut self,
        pivot: NodeId,
        node: NodeId,
    ) -> Result<NodeId, super::HierarchyError> {
        let is_text = |document: &Document, id| matches!(document.node_kind(id), NodeKind::Text(_));
        if is_text(self, node) {
            if let Some(next) = self.next_sibling(pivot).filter(|&next| is_text(self, next)) {
                let copy = self
                    .copy(next, CopyDepth::Deep)
                    .expect("text can be copied");
                self.insert_before(next, copy)?;
                self.detach(next);
            }
        }
        self.insert_after(pivot, node)?;
        self.detach(pivot);
        if !is_text(self, node) {
            return Ok(node);
        }
        let mut merged = node;
        if let Some(previous) = self.previous_sibling(node) {
            if self.merge_text(previous, node) {
                self.detach(node);
                merged = previous;
            }
        }
        if let Some(next) = self.next_sibling(merged) {
            if self.merge_text(merged, next) {
                self.detach(next);
            }
        }
        Ok(merged)
    }

    /// Moves the text of `second` to the end of `first` if both are text
    /// nodes. `second` is always a copy nothing refers to, which libxml2
    /// would free; here it stays behind as an empty slot.
    fn merge_text(&mut self, first: NodeId, second: NodeId) -> bool {
        if !matches!(self.node_kind(first), NodeKind::Text(_)) {
            return false;
        }
        let NodeKind::Text(text) = &mut self.node_mut(second).kind else {
            return false;
        };
        let text = mem::take(text);
        if let NodeKind::Text(first) = &mut self.node_mut(first).kind {
            first.extend_from_slice(&text);
        }
        true
    }

    /// Nokogiri's `relink_namespace` on a reparented node, walking into
    /// elements that have a namespace.
    fn relink_namespaces(&mut self, id: NodeId, namespace_inheritance: bool) {
        let mut pending = vec![id];
        while let Some(node) = pending.pop() {
            let is_element = match self.node_kind(node) {
                NodeKind::Element(_) => true,
                NodeKind::Attribute(_) => false,
                _ => continue,
            };
            self.resolve_prefix(node, is_element);
            let Some(parent) = self.parent(node).filter(|_| is_element) else {
                continue;
            };
            if namespace_inheritance
                && self.namespace(node).is_none()
                && self.namespace(parent).is_some()
            {
                if let NodeKind::Element(parent) = self.node_kind(parent) {
                    let (prefix, ns) = (parent.name.prefix.clone(), parent.name.ns.clone());
                    let local = self
                        .local_name(node)
                        .expect("an element has a name")
                        .clone();
                    self.rename(node, QualName::new(prefix, ns, local));
                }
            }
            self.drop_definitions_in_scope(node, parent);
            if self.namespace(node).is_some() {
                pending.extend_from_slice(self.attributes(node));
                pending.extend(self.children(node));
            }
        }
    }

    /// A name written `prefix:local`, with no namespace or the default one,
    /// takes the namespace the prefix now refers to. Attributes with
    /// `xmlns` stay as they are. libxml2 also gives an element without a
    /// prefix the default namespace in scope, but nothing here defines one.
    fn resolve_prefix(&mut self, id: NodeId, is_element: bool) {
        if self
            .namespace(id)
            .is_some_and(|namespace| namespace.prefix.is_some())
        {
            return;
        }
        let Some(name) = self.local_name(id) else {
            return;
        };
        let Some((prefix, local)) = split_qualified_name(name) else {
            return;
        };
        if !is_element && prefix == "xmlns" {
            return;
        }
        let Some(namespace) = self.search_namespace(id, prefix.as_bytes()) else {
            return;
        };
        let name = QualName::new(
            Some(Prefix::from(prefix)),
            Namespace::from(namespace.href),
            LocalName::from(local),
        );
        self.rename(id, name);
    }

    /// Definitions that the new parent already has in scope, with the same
    /// prefix, are dropped.
    fn drop_definitions_in_scope(&mut self, id: NodeId, parent: NodeId) {
        if self.namespace_definitions(id).is_empty() {
            return;
        }
        let in_scope: Vec<_> = self
            .namespace_scopes(parent)
            .into_iter()
            .map(|scope| (scope.prefix.map(Prefix::from), Namespace::from(scope.href)))
            .collect();
        if let NodeKind::Element(element) = &mut self.node_mut(id).kind {
            element.namespace_definitions.retain(|definition| {
                !in_scope
                    .iter()
                    .any(|(prefix, href)| definition.prefix == *prefix && definition.href == *href)
            });
        }
    }
}

/// Nokogiri's check of the DOM's hierarchy rules, with its additions: text
/// may go directly into a document. Attributes take no children here,
/// since their values are bytes.
fn may_contain(parent: NodeType, child: NodeType) -> bool {
    use NodeType::*;
    let content = matches!(
        child,
        Element | ProcessingInstruction | Comment | Text | CDataSection
    );
    match parent {
        Document | HtmlDocument | DocumentFragment | Element => content,
        _ => false,
    }
}
