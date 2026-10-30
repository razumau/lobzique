#![no_main]

//! Random sequences of tree operations, across an HTML document parsed from
//! the input and an XML document, must leave every link consistent, whether
//! each operation succeeds or is refused, compare nodes consistently, and
//! leave documents that serialize as XML and XHTML.
//!
//! The input is markup, then a zero byte, then operations of four bytes
//! each. The markup is parsed into the HTML document, so namespaces and
//! their definitions come from it, and slices of it give the names, text
//! and fragments the operations use.

use libfuzzer_sys::fuzz_target;
use lobzique_core::dom::{
    CopyDepth, Doctype, Document, DocumentKind, LocalName, NodeId, Position, QualName,
};
use lobzique_core::html5::{parse_document, parse_fragment, Context, ContextNamespace, Options};
use lobzique_core::xml::{self, save_options::*};

const POSITIONS: [Position; 4] = [
    Position::LastChild,
    Position::Before,
    Position::After,
    Position::Replace,
];

fuzz_target!(|data: &[u8]| {
    let (markup, ops) = match data.iter().position(|&byte| byte == 0) {
        Some(end) => (&data[..end], &data[end + 1..]),
        None => (data, &[][..]),
    };
    // Nokogiri's defaults; without a depth limit, deep input is quadratic
    let options = Options {
        max_attributes: Some(400),
        max_errors: Some(0),
        max_tree_depth: Some(400),
        parse_noscript_content_as_text: false,
    };
    let parsed = parse_document(markup, &options).map(|(document, _)| document);
    let mut docs = [
        parsed.unwrap_or_else(|_| Document::new(DocumentKind::Html)),
        Document::new(DocumentKind::Xml),
    ];
    // Up to 31 bytes of the markup, from where `at` points
    let bytes = |at: u8, length: u8| {
        let start = markup.len() * usize::from(at) / 256;
        let end = markup.len().min(start + usize::from(length % 32));
        &markup[start..end]
    };
    let name = |at: u8, length: u8| String::from_utf8_lossy(bytes(at, length)).into_owned();

    for &[kind, x, y, flags] in ops.as_chunks::<4>().0 {
        let [this, other] = &mut docs;
        let (doc, other) = if flags & 1 == 0 {
            (this, other)
        } else {
            (other, this)
        };
        let pick = |doc: &Document, byte: u8| {
            let ids: Vec<NodeId> = doc.node_ids().collect();
            ids[byte as usize % ids.len()]
        };
        let (a, b, foreign) = (pick(doc, x), pick(doc, y), pick(other, y));
        let position = POSITIONS[(flags >> 1) as usize % POSITIONS.len()];
        let depth = CopyDepth::from_level(i32::from(flags >> 3) % 3);
        let inherit = flags & 0x80 != 0;
        match kind % 21 {
            0 => _ = doc.create_element_named(&name(x, y)),
            1 => _ = doc.create_text(bytes(x, y).to_vec()),
            2 => {
                let local = LocalName::from(name(x, y));
                _ = doc.create_attribute(QualName::new(None, Default::default(), local), Vec::new())
            }
            3 => _ = doc.create_fragment(),
            4 => _ = doc.append_child(a, b),
            5 => _ = doc.insert_before(a, b),
            6 => _ = doc.append_attribute(a, b),
            7 => doc.detach(a),
            8 => _ = doc.reparent(a, position, b, inherit),
            9 => _ = doc.reparent_from(a, position, other, foreign, inherit),
            10 => _ = doc.copy(a, depth),
            11 => _ = doc.import(other, foreign, depth),
            12 => doc.set_content(a, bytes(y, flags).to_vec()),
            13 => _ = doc.set_attribute(a, &name(y, flags), bytes(flags, x).to_vec()),
            14 => doc.set_name(a, &name(y, flags)),
            15 => _ = doc.set_root((flags & 0x10 == 0).then_some(a)),
            16 => {
                _ = doc.create_internal_subset(Doctype {
                    name: bytes(x, y).to_vec(),
                    public_id: None,
                    system_id: None,
                })
            }
            17 => doc.set_cdata(a, flags & 0x10 != 0),
            18 => {
                // Enough comparisons to number the nodes, each checked
                // against its reverse
                for id in doc.node_ids().collect::<Vec<_>>() {
                    let order = doc.document_order(a, id);
                    assert_eq!(
                        order.map(|order| order.reverse()),
                        doc.document_order(id, a)
                    );
                }
            }
            19 => {
                let fragment = doc.create_fragment();
                let context = Context::new(ContextNamespace::Html, "body");
                _ = parse_fragment(doc, fragment, bytes(x, y), &context, &options);
            }
            _ => *doc = doc.copy_document(flags & 0x10 == 0),
        }
    }
    for doc in &docs {
        if let Err(violation) = doc.check_invariants() {
            panic!("{violation}");
        }
        for options in [AS_XML | FORMAT, AS_XHTML | FORMAT] {
            _ = xml::write(doc, NodeId::DOCUMENT, None, b"  ", options);
        }
    }
});
