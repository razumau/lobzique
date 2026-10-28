use super::{Document, NodeId, NodeKind};
use html5ever::ns;

impl Document {
    /// Verifies that every link in the arena agrees with its counterpart.
    /// For tests and fuzz targets; linear in the size of the arena.
    pub fn check_invariants(&self) -> Result<(), String> {
        if self.parent(NodeId::DOCUMENT).is_some() {
            return Err("the document node has a parent".into());
        }
        // Set when a node is reached from its parent's children or attributes
        let mut listed = vec![false; self.nodes.len()];
        for id in self.node_ids() {
            if id != NodeId::DOCUMENT && matches!(self.node_kind(id), NodeKind::Document) {
                return Err(format!("{id:?} is a second document node"));
            }
            self.check_children(id, &mut listed)?;
            self.check_attributes(id, &mut listed)?;
        }
        for id in self.node_ids() {
            match self.parent(id) {
                Some(parent) if !listed[id.index()] => {
                    return Err(format!("{id:?} isn't listed by its parent {parent:?}"));
                }
                None if self.previous_sibling(id).is_some() || self.next_sibling(id).is_some() => {
                    return Err(format!("{id:?} has siblings but no parent"));
                }
                _ => {}
            }
        }
        self.check_acyclic()?;
        self.check_names()?;
        self.check_order()
    }

    /// An element in no namespace, in libxml2's terms, is in the namespace
    /// that stands for none in this kind of document.
    fn check_names(&self) -> Result<(), String> {
        let default_namespace = self.default_namespace();
        for id in self.node_ids() {
            let NodeKind::Element(element) = self.node_kind(id) else {
                continue;
            };
            let unqualified = element.name.prefix.is_none()
                && (element.name.ns == ns!() || element.name.ns == ns!(html));
            if unqualified && element.name.ns != default_namespace {
                return Err(format!(
                    "element {id:?} is in {:?}, not this document's {default_namespace:?}",
                    element.name.ns
                ));
            }
        }
        Ok(())
    }

    /// Document order numbered by comparisons is what numbering now gives.
    fn check_order(&self) -> Result<(), String> {
        let Some(order) = self.order.get() else {
            return Ok(());
        };
        let current = self.number_nodes();
        match order
            .iter()
            .zip(&current)
            .position(|(kept, now)| kept != now)
        {
            Some(index) => Err(format!(
                "the kept document order is stale at node index {index}"
            )),
            None => Ok(()),
        }
    }

    fn check_children(&self, id: NodeId, listed: &mut [bool]) -> Result<(), String> {
        let mut previous = None;
        let mut next = self.first_child(id);
        while let Some(child) = next {
            // Also ends a sibling list that loops back on itself
            if std::mem::replace(&mut listed[child.index()], true) {
                return Err(format!("{child:?} is listed twice"));
            }
            if self.parent(child) != Some(id) {
                return Err(format!(
                    "{child:?} is linked under {id:?} but has another parent"
                ));
            }
            if self.previous_sibling(child) != previous {
                return Err(format!("{child:?} has the wrong previous sibling"));
            }
            if matches!(self.node_kind(child), NodeKind::Attribute(_)) {
                return Err(format!("attribute {child:?} is a child of {id:?}"));
            }
            previous = Some(child);
            next = self.next_sibling(child);
        }
        if self.last_child(id) != previous {
            return Err(format!("{id:?} has the wrong last child"));
        }
        if previous.is_some() && !self.node_kind(id).can_have_children() {
            return Err(format!("{id:?} can't have children but has some"));
        }
        Ok(())
    }

    fn check_attributes(&self, id: NodeId, listed: &mut [bool]) -> Result<(), String> {
        for &attribute in self.attributes(id) {
            if !matches!(self.node_kind(attribute), NodeKind::Attribute(_)) {
                return Err(format!("{attribute:?} on {id:?} isn't an attribute"));
            }
            if self.parent(attribute) != Some(id) {
                return Err(format!("attribute {attribute:?} has the wrong parent"));
            }
            if std::mem::replace(&mut listed[attribute.index()], true) {
                return Err(format!("attribute {attribute:?} is listed twice"));
            }
        }
        Ok(())
    }

    /// Follows parent links from every node, remembering which nodes are
    /// known to reach a root, so each link is followed once.
    fn check_acyclic(&self) -> Result<(), String> {
        #[derive(Clone, Copy, PartialEq)]
        enum Walk {
            Unvisited,
            OnPath,
            ReachesRoot,
        }
        let mut walk = vec![Walk::Unvisited; self.nodes.len()];
        let mut path = Vec::new();
        for start in self.node_ids() {
            let mut next = Some(start);
            while let Some(id) = next {
                match walk[id.index()] {
                    Walk::ReachesRoot => break,
                    Walk::OnPath => return Err(format!("{id:?} is its own ancestor")),
                    Walk::Unvisited => {
                        walk[id.index()] = Walk::OnPath;
                        path.push(id);
                        next = self.parent(id);
                    }
                }
            }
            for id in path.drain(..) {
                walk[id.index()] = Walk::ReachesRoot;
            }
        }
        Ok(())
    }
}
