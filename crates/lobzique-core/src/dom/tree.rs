//! Linking and walking nodes. These operations keep the tree well formed and
//! nothing more; Nokogiri's rules for where a node may go, and the copying
//! and text merging that come with them, sit on top.

use super::{Document, NodeId, NodeKind};
use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HierarchyError {
    /// Documents and attributes are never children.
    NotAChild,
    /// Only documents, fragments and elements have children.
    NoChildren,
    /// The new parent is the node itself or one of its descendants.
    Cycle,
    /// Inserting next to a node that has no parent.
    NoParent,
    /// Attributes go on elements only.
    NotAnElement,
    NotAnAttribute,
}

impl fmt::Display for HierarchyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            HierarchyError::NotAChild => "node can't be a child",
            HierarchyError::NoChildren => "node can't have children",
            HierarchyError::Cycle => "node would become its own ancestor",
            HierarchyError::NoParent => "node has no parent",
            HierarchyError::NotAnElement => "attributes go on elements only",
            HierarchyError::NotAnAttribute => "node isn't an attribute",
        })
    }
}

impl std::error::Error for HierarchyError {}

impl Document {
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).parent
    }

    pub fn first_child(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).first_child
    }

    pub fn last_child(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).last_child
    }

    pub fn next_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).next_sibling
    }

    pub fn previous_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).previous_sibling
    }

    /// An element's attribute nodes; empty for every other node.
    pub fn attributes(&self, id: NodeId) -> &[NodeId] {
        match self.node_kind(id) {
            NodeKind::Element(element) => &element.attributes,
            _ => &[],
        }
    }

    pub fn children(&self, id: NodeId) -> Children<'_> {
        Children {
            document: self,
            next: self.first_child(id),
        }
    }

    /// Parent, grandparent and so on, up to the root of the node's tree.
    pub fn ancestors(&self, id: NodeId) -> Ancestors<'_> {
        Ancestors {
            document: self,
            next: self.parent(id),
        }
    }

    /// The node's descendants in document order, without the node itself.
    /// Walks the links rather than recursing, so depth costs no stack.
    pub fn descendants(&self, id: NodeId) -> Descendants<'_> {
        Descendants {
            document: self,
            root: id,
            next: self.first_child(id),
        }
    }

    /// libxml2's `node->next`: the next sibling, or for an attribute the
    /// next attribute of its element.
    pub fn next_in_parent(&self, id: NodeId) -> Option<NodeId> {
        self.next_sibling(id)
            .or_else(|| self.attribute_sibling(id, |index| index.checked_add(1)))
    }

    /// libxml2's `node->prev`, as [`Document::next_in_parent`].
    pub fn previous_in_parent(&self, id: NodeId) -> Option<NodeId> {
        self.previous_sibling(id)
            .or_else(|| self.attribute_sibling(id, |index| index.checked_sub(1)))
    }

    /// libxml2's `xmlNextElementSibling`: none for attributes, documents
    /// and fragments.
    pub fn next_element_sibling(&self, id: NodeId) -> Option<NodeId> {
        if !self.has_element_siblings(id) {
            return None;
        }
        std::iter::successors(self.next_sibling(id), |&sibling| self.next_sibling(sibling))
            .find(|&sibling| self.is_element(sibling))
    }

    /// libxml2's `xmlPreviousElementSibling`, as
    /// [`Document::next_element_sibling`].
    pub fn previous_element_sibling(&self, id: NodeId) -> Option<NodeId> {
        if !self.has_element_siblings(id) {
            return None;
        }
        std::iter::successors(self.previous_sibling(id), |&sibling| {
            self.previous_sibling(sibling)
        })
        .find(|&sibling| self.is_element(sibling))
    }

    pub fn element_children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.children(id).filter(|&child| self.is_element(child))
    }

    pub fn last_element_child(&self, id: NodeId) -> Option<NodeId> {
        std::iter::successors(self.last_child(id), |&child| self.previous_sibling(child))
            .find(|&child| self.is_element(child))
    }

    /// Where `a` is relative to `b` in document order, as libxml2's
    /// `xmlXPathCmpNodes` decides it: an attribute comes after its element
    /// and before the element's children, attributes in their element's
    /// order. `None` for nodes in different trees.
    ///
    /// Comparisons walk the tree until they have cost as much as numbering
    /// every node once, and then use those numbers until the tree changes,
    /// so sorting n nodes stays O(n log n) however wide the tree is.
    pub fn document_order(&self, a: NodeId, b: NodeId) -> Option<Ordering> {
        if a == b {
            return Some(Ordering::Equal);
        }
        if let Some(order) = self.order.get() {
            if let (Some(a), Some(b)) = (order.get(a.index()), order.get(b.index())) {
                return (a.0 == b.0).then(|| a.1.cmp(&b.1));
            }
        }
        let (ordering, cost) = self.walk_order(a, b);
        let walked = self.order_walked.get().saturating_add(cost);
        self.order_walked.set(walked);
        if walked >= self.nodes.len() {
            self.order.get_or_init(|| self.number_nodes());
        }
        ordering
    }

    /// Sorts nodes into document order and drops duplicates, as libxml2
    /// keeps XPath node sets. Nodes of different trees go by their roots'
    /// ids, so the order stays total.
    pub fn sort_in_document_order(&self, nodes: &mut Vec<NodeId>) {
        let root = |id: NodeId| self.ancestors(id).last().unwrap_or(id);
        let mut keyed: Vec<(NodeId, NodeId)> = nodes.iter().map(|&id| (root(id), id)).collect();
        keyed.sort_by(|&(a_root, a), &(b_root, b)| {
            a_root
                .cmp(&b_root)
                .then_with(|| self.document_order(a, b).unwrap_or(Ordering::Equal))
        });
        keyed.dedup_by_key(|&mut (_, id)| id);
        *nodes = keyed.into_iter().map(|(_, id)| id).collect();
    }

    /// [`Document::document_order`] by following links, with the number of
    /// links followed.
    pub(super) fn walk_order(&self, a: NodeId, b: NodeId) -> (Option<Ordering>, usize) {
        // Each node as its tree node, and for an attribute its index there
        let key = |id: NodeId| match (self.node_kind(id), self.parent(id)) {
            (NodeKind::Attribute(_), Some(element)) => {
                let index = self.attributes(element).iter().position(|&a| a == id);
                (element, index)
            }
            _ => (id, None),
        };
        let ((mut a, a_attribute), (mut b, b_attribute)) = (key(a), key(b));
        if a == b {
            return (Some(a_attribute.cmp(&b_attribute)), 0);
        }
        let (a_depth, b_depth) = (self.ancestors(a).count(), self.ancestors(b).count());
        let mut cost = a_depth + b_depth;
        let lift = |mut id: NodeId, steps: usize| {
            for _ in 0..steps {
                id = self.parent(id).expect("counted as an ancestor");
            }
            id
        };
        a = lift(a, a_depth.saturating_sub(b_depth));
        b = lift(b, b_depth.saturating_sub(a_depth));
        if a == b {
            // One is an ancestor of the other, and comes first
            return (Some(a_depth.cmp(&b_depth)), cost);
        }
        loop {
            match (self.parent(a), self.parent(b)) {
                (Some(a_parent), Some(b_parent)) if a_parent == b_parent => break,
                (Some(a_parent), Some(b_parent)) => (a, b) = (a_parent, b_parent),
                _ => return (None, cost),
            }
        }
        let mut sibling = self.next_sibling(a);
        while let Some(next) = sibling {
            cost += 1;
            if next == b {
                return (Some(Ordering::Less), cost);
            }
            sibling = self.next_sibling(next);
        }
        (Some(Ordering::Greater), cost)
    }

    /// Every node's tree root and position in that tree, in document order.
    pub(super) fn number_nodes(&self) -> Vec<(NodeId, u32)> {
        let mut order = vec![(NodeId::DOCUMENT, 0); self.nodes.len()];
        for root in self.node_ids().filter(|&id| self.parent(id).is_none()) {
            // No more than the arena's node count, which fits in a u32
            let mut position = 0;
            for node in std::iter::once(root).chain(self.descendants(root)) {
                for &id in std::iter::once(&node).chain(self.attributes(node)) {
                    order[id.index()] = (root, position);
                    position += 1;
                }
            }
        }
        order
    }

    pub fn append_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), HierarchyError> {
        self.check_insertion(parent, child)?;
        self.detach(child);
        let previous = self.node(parent).last_child;
        self.link(child, parent, previous, None);
        Ok(())
    }

    /// Inserts `node` as the sibling right before `reference`.
    pub fn insert_before(&mut self, reference: NodeId, node: NodeId) -> Result<(), HierarchyError> {
        let parent = self.parent_for_insertion(reference, node)?;
        self.detach(node);
        let previous = self.node(reference).previous_sibling;
        self.link(node, parent, previous, Some(reference));
        Ok(())
    }

    /// Inserts `node` as the sibling right after `reference`.
    pub fn insert_after(&mut self, reference: NodeId, node: NodeId) -> Result<(), HierarchyError> {
        let parent = self.parent_for_insertion(reference, node)?;
        self.detach(node);
        let next = self.node(reference).next_sibling;
        self.link(node, parent, Some(reference), next);
        Ok(())
    }

    pub fn append_attribute(
        &mut self,
        element: NodeId,
        attribute: NodeId,
    ) -> Result<(), HierarchyError> {
        if !matches!(self.node_kind(attribute), NodeKind::Attribute(_)) {
            return Err(HierarchyError::NotAnAttribute);
        }
        if !matches!(self.node_kind(element), NodeKind::Element(_)) {
            return Err(HierarchyError::NotAnElement);
        }
        self.detach(attribute);
        self.tree_changed();
        self.node_mut(attribute).parent = Some(element);
        if let NodeKind::Element(e) = &mut self.node_mut(element).kind {
            e.attributes.push(attribute);
        }
        Ok(())
    }

    /// Unlinks the node from its parent and siblings, or an attribute from
    /// its element. The node and its subtree stay in the arena.
    pub fn detach(&mut self, id: NodeId) {
        let Some(parent) = self.node(id).parent else {
            return;
        };
        self.tree_changed();
        if matches!(self.node_kind(id), NodeKind::Attribute(_)) {
            if let NodeKind::Element(element) = &mut self.node_mut(parent).kind {
                element.attributes.retain(|&a| a != id);
            }
            self.node_mut(id).parent = None;
            return;
        }

        let node = self.node_mut(id);
        let (previous, next) = (node.previous_sibling.take(), node.next_sibling.take());
        node.parent = None;
        match previous {
            Some(previous) => self.node_mut(previous).next_sibling = next,
            None => self.node_mut(parent).first_child = next,
        }
        match next {
            Some(next) => self.node_mut(next).previous_sibling = previous,
            None => self.node_mut(parent).last_child = previous,
        }
    }

    /// Drops the document order that comparisons built.
    fn tree_changed(&mut self) {
        self.order.take();
        *self.order_walked.get_mut() = 0;
    }

    fn attribute_sibling(
        &self,
        id: NodeId,
        step: impl Fn(usize) -> Option<usize>,
    ) -> Option<NodeId> {
        if !matches!(self.node_kind(id), NodeKind::Attribute(_)) {
            return None;
        }
        let attributes = self.attributes(self.parent(id)?);
        let index = attributes.iter().position(|&attribute| attribute == id)?;
        attributes.get(step(index)?).copied()
    }

    fn has_element_siblings(&self, id: NodeId) -> bool {
        !matches!(
            self.node_kind(id),
            NodeKind::Attribute(_) | NodeKind::Document | NodeKind::DocumentFragment
        )
    }

    fn is_element(&self, id: NodeId) -> bool {
        matches!(self.node_kind(id), NodeKind::Element(_))
    }

    fn check_insertion(&self, parent: NodeId, child: NodeId) -> Result<(), HierarchyError> {
        if matches!(
            self.node_kind(child),
            NodeKind::Document | NodeKind::Attribute(_)
        ) {
            return Err(HierarchyError::NotAChild);
        }
        if !self.node_kind(parent).can_have_children() {
            return Err(HierarchyError::NoChildren);
        }
        // A childless node can't be an ancestor, so appending leaves stays O(1)
        let may_be_ancestor = self.first_child(child).is_some();
        if parent == child || may_be_ancestor && self.ancestors(parent).any(|a| a == child) {
            return Err(HierarchyError::Cycle);
        }
        Ok(())
    }

    fn parent_for_insertion(
        &self,
        reference: NodeId,
        node: NodeId,
    ) -> Result<NodeId, HierarchyError> {
        if matches!(self.node_kind(reference), NodeKind::Attribute(_)) {
            return Err(HierarchyError::NotAChild);
        }
        let parent = self.parent(reference).ok_or(HierarchyError::NoParent)?;
        if reference == node {
            return Err(HierarchyError::Cycle);
        }
        self.check_insertion(parent, node)?;
        Ok(parent)
    }

    /// Links a detached node between two adjacent children of `parent`.
    fn link(&mut self, id: NodeId, parent: NodeId, previous: Option<NodeId>, next: Option<NodeId>) {
        self.tree_changed();
        let node = self.node_mut(id);
        node.parent = Some(parent);
        node.previous_sibling = previous;
        node.next_sibling = next;
        match previous {
            Some(previous) => self.node_mut(previous).next_sibling = Some(id),
            None => self.node_mut(parent).first_child = Some(id),
        }
        match next {
            Some(next) => self.node_mut(next).previous_sibling = Some(id),
            None => self.node_mut(parent).last_child = Some(id),
        }
    }
}

pub struct Children<'a> {
    document: &'a Document,
    next: Option<NodeId>,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let id = self.next?;
        self.next = self.document.next_sibling(id);
        Some(id)
    }
}

pub struct Ancestors<'a> {
    document: &'a Document,
    next: Option<NodeId>,
}

impl Iterator for Ancestors<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let id = self.next?;
        self.next = self.document.parent(id);
        Some(id)
    }
}

pub struct Descendants<'a> {
    document: &'a Document,
    root: NodeId,
    next: Option<NodeId>,
}

impl Iterator for Descendants<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let id = self.next?;
        self.next = self.document.first_child(id).or_else(|| {
            let mut node = id;
            loop {
                if node == self.root {
                    return None;
                }
                if let Some(next) = self.document.next_sibling(node) {
                    return Some(next);
                }
                node = self.document.parent(node)?;
            }
        });
        Some(id)
    }
}
