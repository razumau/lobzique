//! libxml2's `xmlSaveTree` (xmlsave.c 2.13) for Nokogiri's
//! `native_write_to`: a node, the node included, as XML or XHTML.
//!
//! libxml2 escapes as it writes and then transcodes the whole stream, with
//! `&#NNN;` for characters the target encoding lacks. [`write`] does the
//! first half and names the encoding for the second.

use crate::dom::{c_string, Doctype, Document, DocumentKind, NodeId, NodeKind};

/// Nokogiri's `SaveOptions`, libxml2's `xmlSaveOption`.
pub mod save_options {
    pub const FORMAT: u32 = 1;
    pub const NO_DECLARATION: u32 = 2;
    pub const NO_EMPTY_TAGS: u32 = 4;
    pub const NO_XHTML: u32 = 8;
    pub const AS_XHTML: u32 = 16;
    pub const AS_XML: u32 = 32;
    pub const AS_HTML: u32 = 64;
}

use save_options::*;

/// What [`write`] wrote, still UTF-8, and the encoding libxml2 would
/// transcode it into, if any.
#[derive(Debug, PartialEq, Eq)]
pub struct Output {
    pub bytes: Vec<u8>,
    pub encoding: Option<Vec<u8>>,
}

/// The options ask for libxml2's HTML serializer, which isn't ported
/// (plan.md decision 3).
#[derive(Debug, PartialEq, Eq)]
pub struct HtmlUnsupported;

/// libxml2's `MAX_INDENT`: the indentation of the deepest levels is cut to
/// as many copies of the indent string as fit in this many bytes.
const MAX_INDENT: usize = 60;

const XHTML_NAMESPACE: &str = "http://www.w3.org/1999/xhtml";

/// `xmlSaveToIO` with `encoding`, `indent` as `xmlTreeIndentString` and
/// `options`, then `xmlSaveTree` on `id`. Walks the tree without recursion.
pub fn write(
    document: &Document,
    id: NodeId,
    encoding: Option<&[u8]>,
    indent: &[u8],
    options: u32,
) -> Result<Output, HtmlUnsupported> {
    let is_document = matches!(document.node_kind(id), NodeKind::Document);
    let html = if options & AS_XHTML != 0 {
        // checked by xmlDocContentDumpOutput only
        is_document && options & AS_HTML != 0
    } else {
        options & AS_HTML != 0 || (document.kind() == DocumentKind::Html && options & AS_XML == 0)
    };
    if html {
        return Err(HtmlUnsupported);
    }
    let mut saver = Saver {
        document,
        out: Vec::new(),
        encoding,
        switched_encoding: None,
        options,
        format: options & FORMAT != 0,
        level: 0,
        indent,
        indent_count: if indent.is_empty() {
            0
        } else {
            MAX_INDENT / indent.len()
        },
        escape: if encoding.is_none() {
            Escape::Entities
        } else {
            Escape::Content
        },
        has_encoding: document.encoding().is_some(),
    };
    saver.node(id, options & AS_XHTML != 0);
    let encoding = encoding.map(<[u8]>::to_vec).or(saver.switched_encoding);
    Ok(Output {
        bytes: saver.out,
        encoding,
    })
}

/// libxml2's escaping function for text: `xmlEscapeEntities` with no
/// encoding, else `xmlEscapeContent`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Escape {
    Entities,
    Content,
}

/// libxml2's `xmlSaveCtxt`.
struct Saver<'a> {
    document: &'a Document,
    out: Vec<u8>,
    encoding: Option<&'a [u8]>,
    switched_encoding: Option<Vec<u8>>,
    options: u32,
    format: bool,
    level: usize,
    indent: &'a [u8],
    indent_count: usize,
    escape: Escape,
    /// `doc->encoding != NULL`, which keeps attribute values from being
    /// written as character references.
    has_encoding: bool,
}

impl Saver<'_> {
    /// `xmlNodeDumpOutputInternal`, or `xhtmlNodeDumpOutput` with `xhtml`.
    /// The two differ only where `xhtml` is read.
    fn node(&mut self, root: NodeId, xhtml: bool) {
        let document = self.document;
        if xhtml {
            // never restored: xhtmlNodeDumpOutput returns before it would be
            self.options |= AS_XHTML;
        }
        let format = self.format;
        let mut unformatted = None;
        let mut parent = document.parent(root);
        let mut current = root;
        loop {
            let mut descend = false;
            match document.node_kind(current) {
                NodeKind::Document => self.document_content(),
                NodeKind::Doctype(doctype) => self.doctype(doctype),
                NodeKind::DocumentFragment => descend = document.first_child(current).is_some(),
                NodeKind::Element(element) => {
                    if current != root && self.format {
                        self.indent(self.level);
                    }
                    let prefix = self.document.namespace(current).and_then(|ns| ns.prefix);
                    self.out.push(b'<');
                    self.qualified_name(prefix, &element.name.local);
                    for definition in &element.namespace_definitions {
                        self.namespace_definition(definition.prefix.as_deref(), &definition.href);
                    }
                    let mut add_meta = false;
                    if xhtml {
                        if &*element.name.local == "html"
                            && document.namespace(current).is_none()
                            && element.namespace_definitions.is_empty()
                        {
                            self.out.extend_from_slice(b" xmlns=\"");
                            self.out.extend_from_slice(XHTML_NAMESPACE.as_bytes());
                            self.out.push(b'"');
                        }
                        self.xhtml_attributes(current, &element.attributes);
                        add_meta = self.lacks_content_type(current, parent);
                    } else {
                        for &attribute in &element.attributes {
                            self.attribute(attribute);
                        }
                    }
                    let has_children = document.first_child(current).is_some();
                    if !has_children && xhtml {
                        if prefix.is_none() && !add_meta && self.is_empty_xhtml(current) {
                            self.out.extend_from_slice(b" />");
                        } else {
                            self.out.push(b'>');
                            if add_meta {
                                self.content_type_meta(true);
                            }
                            self.end_tag(prefix, &element.name.local);
                        }
                    } else if !has_children {
                        if self.options & NO_EMPTY_TAGS == 0 {
                            self.out.extend_from_slice(b"/>");
                        } else {
                            self.out.push(b'>');
                            self.end_tag(prefix, &element.name.local);
                        }
                    } else {
                        if xhtml {
                            self.out.push(b'>');
                            if add_meta {
                                self.content_type_meta(false);
                            }
                        }
                        if self.format
                            && document.children(current).any(|child| {
                                matches!(document.node_kind(child), NodeKind::Text(_))
                                    || (!xhtml
                                        && matches!(document.node_kind(child), NodeKind::CData(_)))
                            })
                        {
                            self.format = false;
                            unformatted = Some(current);
                        }
                        if !xhtml {
                            self.out.push(b'>');
                        }
                        if self.format {
                            self.out.push(b'\n');
                        }
                        self.level += 1;
                        descend = true;
                    }
                }
                NodeKind::Text(text) => self.text(c_string(text)),
                NodeKind::ProcessingInstruction(pi) => {
                    if !xhtml && current != root && self.format {
                        self.indent(self.level);
                    }
                    self.out.extend_from_slice(b"<?");
                    self.out.extend_from_slice(c_string(&pi.target));
                    self.out.push(b' ');
                    self.out.extend_from_slice(c_string(&pi.data));
                    self.out.extend_from_slice(b"?>");
                }
                NodeKind::Comment(text) => {
                    if !xhtml && current != root && self.format {
                        self.indent(self.level);
                    }
                    self.out.extend_from_slice(b"<!--");
                    self.out.extend_from_slice(c_string(text));
                    self.out.extend_from_slice(b"-->");
                }
                NodeKind::CData(text) => self.cdata(c_string(text)),
                NodeKind::Attribute(_) => self.attribute(current),
            }
            if descend {
                parent = Some(current);
                current = document
                    .first_child(current)
                    .expect("descending into children");
                continue;
            }
            loop {
                if current == root {
                    return;
                }
                if self.format {
                    self.out.push(b'\n');
                }
                if let Some(next) = document.next_sibling(current) {
                    current = next;
                    break;
                }
                current = parent.expect("a node below the root has a parent");
                parent = document.parent(current);
                if let NodeKind::Element(element) = document.node_kind(current) {
                    self.level = self.level.saturating_sub(1);
                    if self.format {
                        self.indent(self.level);
                    }
                    let prefix = self.document.namespace(current).and_then(|ns| ns.prefix);
                    self.end_tag(prefix, &element.name.local);
                    if unformatted == Some(current) {
                        self.format = format;
                        unformatted = None;
                    }
                }
            }
        }
    }

    /// `xmlDocContentDumpOutput`, for the XML declaration and the
    /// document's children.
    fn document_content(&mut self) {
        let document = self.document;
        let had_encoding = self.has_encoding;
        let encoding = match self.encoding {
            Some(encoding) => {
                self.has_encoding = true;
                Some(encoding)
            }
            None => document.encoding(),
        };
        if let (Some(encoding), None) = (encoding, self.encoding) {
            if self.options & NO_DECLARATION == 0 {
                // xmlParseCharEncoding: only UTF-8 and no encoding go untranscoded
                let utf8 = encoding.is_empty()
                    || encoding.eq_ignore_ascii_case(b"UTF-8")
                    || encoding.eq_ignore_ascii_case(b"UTF8");
                if !utf8 {
                    self.switched_encoding = Some(encoding.to_vec());
                }
                self.escape = Escape::Content;
            }
        }
        if self.options & NO_DECLARATION == 0 {
            self.out.extend_from_slice(b"<?xml version=");
            self.quoted(document.version().unwrap_or(b"1.0"));
            if let Some(encoding) = encoding {
                self.out.extend_from_slice(b" encoding=");
                self.quoted(encoding);
            }
            // htmlNewDocNoDtD makes HTML documents standalone, xmlNewDoc leaves it unset
            if document.kind() == DocumentKind::Html {
                self.out.extend_from_slice(b" standalone=\"yes\"");
            }
            self.out.extend_from_slice(b"?>\n");
        }
        let mut xhtml = self.options & AS_XHTML != 0;
        if self.options & NO_XHTML == 0 {
            if let Some(doctype) = document.doctype() {
                if let NodeKind::Doctype(doctype) = document.node_kind(doctype) {
                    xhtml = is_xhtml(doctype);
                }
            }
        }
        for child in document.children(NodeId::DOCUMENT) {
            self.level = 0;
            self.node(child, xhtml);
            self.out.push(b'\n');
        }
        self.has_encoding = had_encoding;
    }

    /// `xmlDtdDumpOutput` for a doctype, which declares nothing.
    fn doctype(&mut self, doctype: &Doctype) {
        self.out.extend_from_slice(b"<!DOCTYPE ");
        self.out.extend_from_slice(c_string(&doctype.name));
        if let Some(public_id) = &doctype.public_id {
            self.out.extend_from_slice(b" PUBLIC ");
            self.quoted(public_id);
            self.out.push(b' ');
            self.quoted(doctype.system_id.as_deref().unwrap_or_default());
        } else if let Some(system_id) = &doctype.system_id {
            self.out.extend_from_slice(b" SYSTEM ");
            self.quoted(system_id);
        }
        self.out.push(b'>');
    }

    /// `xmlAttrDumpOutput`.
    fn attribute(&mut self, id: NodeId) {
        let NodeKind::Attribute(attribute) = self.document.node_kind(id) else {
            return;
        };
        let namespace = self.document.namespace(id);
        self.out.push(b' ');
        self.qualified_name(namespace.and_then(|ns| ns.prefix), &attribute.name.local);
        self.out.extend_from_slice(b"=\"");
        let value = c_string(&attribute.value);
        if self.options & AS_XHTML != 0
            && namespace.is_none()
            && value.is_empty()
            && is_boolean_attribute(&attribute.name.local)
        {
            self.out.extend_from_slice(attribute.name.local.as_bytes());
        } else {
            self.attribute_value(value);
        }
        self.out.push(b'"');
    }

    /// `xhtmlAttrListDumpOutput`: the attributes, then an `id` for a
    /// `name` and the missing one of `lang` and `xml:lang` (XHTML 1.0
    /// appendix C.8 and C.7).
    fn xhtml_attributes(&mut self, element: NodeId, attributes: &[NodeId]) {
        let document = self.document;
        let (mut id, mut name, mut lang, mut xml_lang) = (None, None, None, None);
        for &attribute in attributes {
            if let NodeKind::Attribute(attr) = document.node_kind(attribute) {
                let namespace = document.namespace(attribute);
                let value = c_string(&attr.value);
                match (&*attr.name.local, namespace) {
                    ("id", None) => id = Some(value),
                    ("name", None) => name = Some(value),
                    ("lang", None) => lang = Some(value),
                    ("lang", Some(ns)) if ns.prefix == Some("xml") => xml_lang = Some(value),
                    _ => {}
                }
            }
            self.attribute(attribute);
        }
        if let (Some(name), None) = (name, id) {
            let named = matches!(
                document.node_kind(element),
                NodeKind::Element(e) if matches!(
                    &*e.name.local,
                    "a" | "p" | "div" | "img" | "map" | "applet" | "form" | "frame" | "iframe"
                )
            );
            if named {
                self.out.extend_from_slice(b" id=\"");
                self.attribute_value(name);
                self.out.push(b'"');
            }
        }
        match (lang, xml_lang) {
            (Some(lang), None) => {
                self.out.extend_from_slice(b" xml:lang=\"");
                self.attribute_value(lang);
                self.out.push(b'"');
            }
            (None, Some(xml_lang)) => {
                self.out.extend_from_slice(b" lang=\"");
                self.attribute_value(xml_lang);
                self.out.push(b'"');
            }
            _ => {}
        }
    }

    /// Whether a `head` that is the document's `html`'s child has no
    /// Content-Type `meta`, which XHTML output then adds.
    fn lacks_content_type(&self, head: NodeId, parent: Option<NodeId>) -> bool {
        let document = self.document;
        let named = |id: NodeId, name: &str| matches!(document.node_kind(id), NodeKind::Element(e) if &*e.name.local == name);
        let Some(parent) = parent else {
            return false;
        };
        document.parent(parent) == Some(NodeId::DOCUMENT)
            && named(head, "head")
            && named(parent, "html")
            && !document.children(head).any(|child| {
                named(child, "meta")
                    && document
                        .attribute_named(child, b"http-equiv")
                        .and_then(|attribute| match document.node_kind(attribute) {
                            NodeKind::Attribute(attribute) => Some(c_string(&attribute.value)),
                            _ => None,
                        })
                        .is_some_and(|value| value.eq_ignore_ascii_case(b"Content-Type"))
            })
    }

    /// The `meta` XHTML output adds to a `head`, which has children unless
    /// `empty`.
    fn content_type_meta(&mut self, empty: bool) {
        if self.format {
            self.out.push(b'\n');
            self.indent(self.level + 1);
        }
        self.out
            .extend_from_slice(b"<meta http-equiv=\"Content-Type\" content=\"text/html; charset=");
        self.out
            .extend_from_slice(self.encoding.unwrap_or(b"UTF-8"));
        self.out.extend_from_slice(b"\" />");
        if empty && self.format {
            self.out.push(b'\n');
        }
    }

    /// `xhtmlIsEmpty`: an element XHTML output closes with ` />`.
    fn is_empty_xhtml(&self, id: NodeId) -> bool {
        if self
            .document
            .namespace(id)
            .is_some_and(|ns| ns.href != XHTML_NAMESPACE)
        {
            return false;
        }
        matches!(
            self.document.node_kind(id),
            NodeKind::Element(e) if matches!(
                &*e.name.local,
                "area" | "br" | "base" | "basefont" | "col" | "frame" | "hr" | "img" | "input"
                    | "isindex" | "link" | "meta" | "param"
            )
        )
    }

    /// `xmlNsDumpOutput`.
    fn namespace_definition(&mut self, prefix: Option<&str>, href: &str) {
        match prefix {
            Some("xml") => return,
            Some(prefix) => {
                self.out.extend_from_slice(b" xmlns:");
                self.out.extend_from_slice(prefix.as_bytes());
            }
            None => self.out.extend_from_slice(b" xmlns"),
        }
        self.out.extend_from_slice(b"=\"");
        self.attribute_value(href.as_bytes());
        self.out.push(b'"');
    }

    fn qualified_name(&mut self, prefix: Option<&str>, local: &str) {
        if let Some(prefix) = prefix {
            self.out.extend_from_slice(prefix.as_bytes());
            self.out.push(b':');
        }
        self.out.extend_from_slice(local.as_bytes());
    }

    fn end_tag(&mut self, prefix: Option<&str>, local: &str) {
        self.out.extend_from_slice(b"</");
        self.qualified_name(prefix, local);
        self.out.push(b'>');
    }

    fn indent(&mut self, level: usize) {
        for _ in 0..level.min(self.indent_count) {
            self.out.extend_from_slice(self.indent);
        }
    }

    /// `xmlOutputBufferWriteEscape` with the context's escaping function.
    fn text(&mut self, text: &[u8]) {
        let mut index = 0;
        while index < text.len() {
            let byte = text[index];
            index += 1;
            let replacement: &[u8] = match byte {
                b'<' => b"&lt;",
                b'>' => b"&gt;",
                b'&' => b"&amp;",
                b'\r' if self.escape == Escape::Entities => b"&#xD;",
                b'\r' => b"&#13;",
                _ if self.escape == Escape::Content => {
                    self.out.push(byte);
                    continue;
                }
                0x20.. if byte < 0x80 => {
                    self.out.push(byte);
                    continue;
                }
                b'\n' | b'\t' => {
                    self.out.push(byte);
                    continue;
                }
                ..0x80 => {
                    hex_character_reference(&mut self.out, 0xFFFD);
                    continue;
                }
                _ => {
                    let (character, length) = decode_utf8(&text[index - 1..]);
                    index += length - 1;
                    hex_character_reference(&mut self.out, character);
                    continue;
                }
            };
            self.out.extend_from_slice(replacement);
        }
    }

    /// `xmlBufAttrSerializeTxtContent`. With no encoding in effect, other
    /// than ASCII is written as character references, except a last byte.
    fn attribute_value(&mut self, value: &[u8]) {
        let mut index = 0;
        while index < value.len() {
            let byte = value[index];
            let replacement: &[u8] = match byte {
                b'\n' => b"&#10;",
                b'\r' => b"&#13;",
                b'\t' => b"&#9;",
                b'"' => b"&quot;",
                b'<' => b"&lt;",
                b'>' => b"&gt;",
                b'&' => b"&amp;",
                0x80.. if index + 1 < value.len() && !self.has_encoding => {
                    let (character, length) = decode_utf8(&value[index..]);
                    index += length;
                    hex_character_reference(&mut self.out, character);
                    continue;
                }
                _ => {
                    self.out.push(byte);
                    index += 1;
                    continue;
                }
            };
            self.out.extend_from_slice(replacement);
            index += 1;
        }
    }

    /// A CDATA section, split after each `]]` that a `>` follows.
    fn cdata(&mut self, text: &[u8]) {
        if text.is_empty() {
            self.out.extend_from_slice(b"<![CDATA[]]>");
            return;
        }
        let mut start = 0;
        let mut end = 0;
        while end < text.len() {
            if text[end..].starts_with(b"]]>") {
                end += 2;
                self.out.extend_from_slice(b"<![CDATA[");
                self.out.extend_from_slice(&text[start..end]);
                self.out.extend_from_slice(b"]]>");
                start = end;
            }
            end += 1;
        }
        if start != end {
            self.out.extend_from_slice(b"<![CDATA[");
            self.out.extend_from_slice(&text[start..]);
            self.out.extend_from_slice(b"]]>");
        }
    }

    /// `xmlOutputBufferWriteQuotedString`.
    fn quoted(&mut self, string: &[u8]) {
        let string = c_string(string);
        if !string.contains(&b'"') {
            self.out.push(b'"');
            self.out.extend_from_slice(string);
            self.out.push(b'"');
        } else if !string.contains(&b'\'') {
            self.out.push(b'\'');
            self.out.extend_from_slice(string);
            self.out.push(b'\'');
        } else {
            self.out.push(b'"');
            for &byte in string {
                if byte == b'"' {
                    self.out.extend_from_slice(b"&quot;");
                } else {
                    self.out.push(byte);
                }
            }
            self.out.push(b'"');
        }
    }
}

/// `xmlIsXHTML`: whether a doctype names one of XHTML 1.0's DTDs.
fn is_xhtml(doctype: &Doctype) -> bool {
    const PUBLIC_IDS: [&[u8]; 3] = [
        b"-//W3C//DTD XHTML 1.0 Strict//EN",
        b"-//W3C//DTD XHTML 1.0 Frameset//EN",
        b"-//W3C//DTD XHTML 1.0 Transitional//EN",
    ];
    const SYSTEM_IDS: [&[u8]; 3] = [
        b"http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd",
        b"http://www.w3.org/TR/xhtml1/DTD/xhtml1-frameset.dtd",
        b"http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd",
    ];
    let listed = |id: &Option<Vec<u8>>, ids: &[&[u8]]| {
        id.as_deref().is_some_and(|id| ids.contains(&c_string(id)))
    };
    listed(&doctype.public_id, &PUBLIC_IDS) || listed(&doctype.system_id, &SYSTEM_IDS)
}

/// `htmlIsBooleanAttr`, which ignores case.
fn is_boolean_attribute(name: &str) -> bool {
    [
        "checked", "compact", "declare", "defer", "disabled", "ismap", "multiple", "nohref",
        "noresize", "noshade", "nowrap", "readonly", "selected",
    ]
    .iter()
    .any(|boolean| boolean.eq_ignore_ascii_case(name))
}

/// `xmlGetUTF8Char` on the bytes from a non-ASCII one: the character and
/// how many bytes it took, or U+FFFD and one byte for a malformed sequence.
/// Characters XML doesn't allow are U+FFFD too.
fn decode_utf8(bytes: &[u8]) -> (u32, usize) {
    let length = match bytes[0] {
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xFF => 4,
        _ => return (0xFFFD, 1),
    };
    let Some(sequence) = bytes.get(..length) else {
        return (0xFFFD, 1);
    };
    if sequence[1..].iter().any(|&byte| byte & 0xC0 != 0x80) {
        return (0xFFFD, 1);
    }
    let lead = u32::from(sequence[0]) & (0x7F >> length);
    let character = sequence[1..].iter().fold(lead, |character, &byte| {
        character << 6 | u32::from(byte & 0x3F)
    });
    let valid = match length {
        3 => character >= 0x800 && !(0xD800..0xE000).contains(&character),
        4 => (0x10000..0x110000).contains(&character),
        _ => true,
    };
    if !valid {
        return (0xFFFD, 1);
    }
    // IS_CHAR, past ASCII
    let character = if (0xFFFE..0x10000).contains(&character) {
        0xFFFD
    } else {
        character
    };
    (character, length)
}

/// `xmlSerializeHexCharRef`: uppercase, without leading zeros.
fn hex_character_reference(out: &mut Vec<u8>, character: u32) {
    out.extend_from_slice(format!("&#x{character:X};").as_bytes());
}
