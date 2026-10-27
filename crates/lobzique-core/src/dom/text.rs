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

    /// libxml2's `xmlIsBlankNode`: a text or CDATA node of nothing but
    /// spaces, tabs and line breaks.
    pub fn is_blank(&self, id: NodeId) -> bool {
        match self.node_kind(id) {
            NodeKind::Text(text) | NodeKind::CData(text) => text
                .iter()
                .all(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r')),
            _ => false,
        }
    }

    /// Nokogiri's `native_content=`, `xmlNodeSetContent` after unlinking
    /// the children: elements, fragments and attributes get the content
    /// with its character references decoded, character data and
    /// processing instructions get it as is, and other nodes get nothing.
    pub fn set_content(&mut self, id: NodeId, content: Vec<u8>) {
        while let Some(child) = self.first_child(id) {
            self.detach(child);
        }
        if matches!(
            self.node_kind(id),
            NodeKind::Element(_) | NodeKind::DocumentFragment
        ) {
            let text = decode_references(&content);
            if !text.is_empty() {
                let text = self.create_text(text);
                self.append_child(id, text)
                    .expect("an element takes a text child");
            }
            return;
        }
        match &mut self.node_mut(id).kind {
            NodeKind::Attribute(attribute) => attribute.value = decode_references(&content),
            NodeKind::Text(text) | NodeKind::CData(text) | NodeKind::Comment(text) => {
                *text = content
            }
            NodeKind::ProcessingInstruction(pi) => pi.data = content,
            NodeKind::Element(_)
            | NodeKind::DocumentFragment
            | NodeKind::Document
            | NodeKind::Doctype(_) => {}
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

/// What libxml2's `xmlNodeSetContent` decodes: the predefined entities and
/// character references, which covers everything
/// [`encode_special_chars`] writes. libxml2 makes any other entity
/// reference a node of its own; here it stays text.
fn decode_references(input: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.iter().position(|&byte| byte == b'&') {
        decoded.extend_from_slice(&rest[..start]);
        rest = &rest[start..];
        let reference = rest
            .iter()
            .position(|&byte| byte == b';')
            .and_then(|end| Some((decode_reference(&rest[1..end])?, end)));
        match reference {
            Some((character, end)) => {
                decoded.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
                rest = &rest[end + 1..];
            }
            None => {
                decoded.push(b'&');
                rest = &rest[1..];
            }
        }
    }
    decoded.extend_from_slice(rest);
    decoded
}

/// The character `&name;` stands for, given `name`.
fn decode_reference(name: &[u8]) -> Option<char> {
    let number = |digits: &[u8], radix| {
        let digits = std::str::from_utf8(digits).ok()?;
        let all_digits = !digits.is_empty() && digits.chars().all(|c| c.is_digit(radix));
        all_digits
            .then(|| u32::from_str_radix(digits, radix).ok())
            .flatten()
            .and_then(char::from_u32)
            .filter(|&c| c != '\0')
    };
    match name {
        b"lt" => Some('<'),
        b"gt" => Some('>'),
        b"amp" => Some('&'),
        b"quot" => Some('"'),
        b"apos" => Some('\''),
        [b'#', b'x' | b'X', hex @ ..] => number(hex, 16),
        [b'#', decimal @ ..] => number(decimal, 10),
        _ => None,
    }
}

/// libxml2's `xmlEncodeSpecialChars`, behind Nokogiri's
/// `Node#encode_special_chars` and `Node#content=`.
pub fn encode_special_chars(input: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(input.len());
    for &byte in input {
        match byte {
            b'<' => encoded.extend_from_slice(b"&lt;"),
            b'>' => encoded.extend_from_slice(b"&gt;"),
            b'&' => encoded.extend_from_slice(b"&amp;"),
            b'"' => encoded.extend_from_slice(b"&quot;"),
            b'\r' => encoded.extend_from_slice(b"&#13;"),
            _ => encoded.push(byte),
        }
    }
    encoded
}
