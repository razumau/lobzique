use super::{Document, NodeId, NodeKind};

impl Document {
    /// Verifies that every link in the arena agrees with its counterpart.
    /// For tests and fuzz targets; it walks the whole arena.
    pub fn check_invariants(&self) -> Result<(), String> {
        if self.parent(NodeId::DOCUMENT).is_some() {
            return Err("the document node has a parent".into());
        }
        for index in 0..self.nodes.len() {
            let id = NodeId::from_index(index);
            if index > 0 && matches!(self.node_kind(id), NodeKind::Document) {
                return Err(format!("{id:?} is a second document node"));
            }
            self.check_children(id)?;
            self.check_attributes(id)?;
            if self.ancestors(id).take(self.nodes.len()).any(|a| a == id) {
                return Err(format!("{id:?} is its own ancestor"));
            }
        }
        Ok(())
    }

    fn check_children(&self, id: NodeId) -> Result<(), String> {
        let mut previous = None;
        for child in self.children(id).take(self.nodes.len() + 1) {
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
        }
        if self.last_child(id) != previous {
            return Err(format!("{id:?} has the wrong last child"));
        }
        if previous.is_some() && !self.node_kind(id).can_have_children() {
            return Err(format!("{id:?} can't have children but has some"));
        }
        if let Some(parent) = self.parent(id) {
            let listed = match self.node_kind(id) {
                NodeKind::Attribute(_) => self.attributes(parent).contains(&id),
                _ => self.children(parent).any(|c| c == id),
            };
            if !listed {
                return Err(format!("{id:?} isn't listed by its parent {parent:?}"));
            }
        } else if self.previous_sibling(id).is_some() || self.next_sibling(id).is_some() {
            return Err(format!("{id:?} has siblings but no parent"));
        }
        Ok(())
    }

    fn check_attributes(&self, id: NodeId) -> Result<(), String> {
        let attributes = self.attributes(id);
        for (i, &attribute) in attributes.iter().enumerate() {
            if !matches!(self.node_kind(attribute), NodeKind::Attribute(_)) {
                return Err(format!("{attribute:?} on {id:?} isn't an attribute"));
            }
            if self.parent(attribute) != Some(id) {
                return Err(format!("attribute {attribute:?} has the wrong parent"));
            }
            if attributes[..i].contains(&attribute) {
                return Err(format!("attribute {attribute:?} is listed twice"));
            }
        }
        Ok(())
    }
}
