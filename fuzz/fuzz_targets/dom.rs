#![no_main]

//! Random sequences of tree operations must leave every link consistent,
//! whether each operation succeeds or is refused.

use libfuzzer_sys::fuzz_target;
use lobzique_core::dom::{Document, DocumentKind, LocalName, NodeId, QualName};

fuzz_target!(|ops: &[u8]| {
    let mut doc = Document::new(DocumentKind::Html);
    let name = |local: &str| QualName::new(None, Default::default(), LocalName::from(local));
    for op in ops.chunks_exact(3) {
        let ids: Vec<NodeId> = doc.node_ids().collect();
        let a = ids[op[1] as usize % ids.len()];
        let b = ids[op[2] as usize % ids.len()];
        let _ = match op[0] % 9 {
            0 => Ok(drop(doc.create_element(name("e")))),
            1 => Ok(drop(doc.create_text("t".into()))),
            2 => Ok(drop(doc.create_attribute(name("a"), Vec::new()))),
            3 => Ok(drop(doc.create_fragment())),
            4 => doc.append_child(a, b),
            5 => doc.insert_before(a, b),
            6 => doc.insert_after(a, b),
            7 => doc.append_attribute(a, b),
            _ => Ok(doc.detach(a)),
        };
    }
    if let Err(violation) = doc.check_invariants() {
        panic!("{violation}");
    }
});
