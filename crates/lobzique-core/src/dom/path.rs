use super::{Document, NamespaceRef, NodeId, NodeKind};

impl Document {
    /// libxml2's `xmlGetNodePath`, which Nokogiri's `Node#path` returns: a
    /// step per ancestor, numbered when siblings share the step's name.
    /// `None` where libxml2 gives up, at a doctype or a fragment, which
    /// Nokogiri shows as `"?"`; nodes in a fragment have no path either.
    pub fn path(&self, id: NodeId) -> Option<Vec<u8>> {
        let mut steps = Vec::new();
        let mut next = Some(id);
        while let Some(node) = next {
            next = self.parent(node);
            let (separator, name, position): (&[u8], _, _) = match self.node_kind(node) {
                NodeKind::Document if steps.is_empty() => (b"/", Vec::new(), None),
                NodeKind::Document => break,
                NodeKind::Element(element) => {
                    let local = &element.name.local;
                    let namespace = self.namespace(node);
                    let name = match namespace {
                        Some(NamespaceRef {
                            prefix: Some(prefix),
                            ..
                        }) => qualified(prefix, local),
                        // libxml2 can't name an element in a default
                        // namespace, so it counts every element as a match
                        Some(_) => b"*".to_vec(),
                        None => local.as_bytes().to_vec(),
                    };
                    let any_element = namespace.is_some_and(|namespace| namespace.prefix.is_none());
                    let prefix = namespace.map(|namespace| namespace.prefix);
                    let position = self.position(node, |other| match self.node_kind(other) {
                        NodeKind::Element(other_element) => {
                            any_element
                                || other_element.name.local == *local
                                    && self.namespace(other).map(|namespace| namespace.prefix)
                                        == prefix
                        }
                        _ => false,
                    });
                    (b"/", name, position)
                }
                NodeKind::Comment(_) => {
                    let position = self.position(node, |other| {
                        matches!(self.node_kind(other), NodeKind::Comment(_))
                    });
                    (b"/", b"comment()".to_vec(), position)
                }
                NodeKind::Text(_) | NodeKind::CData(_) => {
                    let position = self.position(node, |other| {
                        matches!(
                            self.node_kind(other),
                            NodeKind::Text(_) | NodeKind::CData(_)
                        )
                    });
                    (b"/", b"text()".to_vec(), position)
                }
                NodeKind::ProcessingInstruction(pi) => {
                    let mut name = b"processing-instruction('".to_vec();
                    name.extend_from_slice(&pi.target);
                    name.extend_from_slice(b"')");
                    let position = self.position(node, |other| {
                        matches!(self.node_kind(other), NodeKind::ProcessingInstruction(other) if other.target == pi.target)
                    });
                    (b"/", truncated(name), position)
                }
                NodeKind::Attribute(attribute) => {
                    let local = &attribute.name.local;
                    let name = match self.namespace(node) {
                        Some(namespace) => match namespace.prefix {
                            Some(prefix) => qualified(prefix, local),
                            None => truncated(local.as_bytes().to_vec()),
                        },
                        None => local.as_bytes().to_vec(),
                    };
                    (b"/@", name, None)
                }
                NodeKind::Doctype(_) | NodeKind::DocumentFragment => return None,
            };
            steps.push((separator, name, position));
        }

        let mut path = Vec::new();
        for (separator, name, position) in steps.into_iter().rev() {
            path.extend_from_slice(separator);
            path.extend_from_slice(&name);
            if let Some(position) = position {
                path.extend_from_slice(format!("[{position}]").as_bytes());
            }
        }
        Some(path)
    }

    /// The node's position among the siblings `same_step` accepts, or
    /// `None` if it is the only one.
    fn position(&self, id: NodeId, same_step: impl Fn(NodeId) -> bool) -> Option<usize> {
        let mut before = 0;
        let mut previous = self.previous_sibling(id);
        while let Some(sibling) = previous {
            before += usize::from(same_step(sibling));
            previous = self.previous_sibling(sibling);
        }
        let mut following =
            std::iter::successors(self.next_sibling(id), |&sibling| self.next_sibling(sibling));
        (before > 0 || following.any(same_step)).then_some(before + 1)
    }
}

fn qualified(prefix: &str, local: &str) -> Vec<u8> {
    truncated(format!("{prefix}:{local}").into_bytes())
}

/// libxml2 formats generated step names into a 100-byte buffer.
fn truncated(mut name: Vec<u8>) -> Vec<u8> {
    name.truncate(98);
    name
}
