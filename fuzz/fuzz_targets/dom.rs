#![no_main]

//! Random sequences of tree operations, across two documents, must leave
//! every link consistent, whether each operation succeeds or is refused.

use libfuzzer_sys::fuzz_target;
use lobzique_core::dom::{
    CopyDepth, Document, DocumentKind, LocalName, NodeId, Position, QualName,
};

const POSITIONS: [Position; 4] = [
    Position::LastChild,
    Position::Before,
    Position::After,
    Position::Replace,
];

fuzz_target!(|ops: &[u8]| {
    let mut docs = [
        Document::new(DocumentKind::Html),
        Document::new(DocumentKind::Xml),
    ];
    let name = |local: &str| QualName::new(None, Default::default(), LocalName::from(local));
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
        match kind % 16 {
            0 => _ = doc.create_element(name("e")),
            1 => _ = doc.create_element_named("svg:e"),
            2 => _ = doc.create_text("t".into()),
            3 => _ = doc.create_attribute(name("a"), Vec::new()),
            4 => _ = doc.create_fragment(),
            5 => _ = doc.append_child(a, b),
            6 => _ = doc.insert_before(a, b),
            7 => _ = doc.append_attribute(a, b),
            8 => doc.detach(a),
            9 => _ = doc.reparent(a, position, b, flags & 0x80 != 0),
            10 => _ = doc.reparent_from(a, position, other, foreign, false),
            11 => _ = doc.copy(a, depth),
            12 => _ = doc.import(other, foreign, depth),
            13 => doc.set_content(a, b"x&lt;".to_vec()),
            14 => _ = doc.set_attribute(a, "xml:lang", Vec::new()),
            _ => *doc = doc.copy_document(flags & 1 == 0),
        }
    }
    for doc in &docs {
        if let Err(violation) = doc.check_invariants() {
            panic!("{violation}");
        }
    }
});
