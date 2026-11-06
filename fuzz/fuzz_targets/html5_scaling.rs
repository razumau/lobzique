#![no_main]

//! Parsing and serializing must cost about the same per copy of the input
//! however many copies there are. `-timeout` catches only catastrophic
//! slowness: a quadratic path that takes 2s on 64 KB passes it. So the
//! input is repeated K, 2K and 4K times and the core counts the work each
//! run does (`lobzique_core::work`); going from 2K to 4K copies may cost at
//! most RATIO times what going from K to 2K did, where linear work costs 2
//! and quadratic 4.
//!
//! Elements a copy leaves open nest the next copy inside them, and each
//! insertion walks up the tree to check the depth limit, as html5ever walks
//! its stack of open elements, so the work is linear in the input times the
//! tree's depth. The allowance grows with the depth, which the depth limit
//! bounds. The first byte picks the options, as in the html5 target.

use libfuzzer_sys::fuzz_target;
use lobzique_core::dom::{Document, DocumentKind, NodeId};
use lobzique_core::html5::{
    parse_document, parse_fragment, serialize, Context, ContextNamespace, Options,
};
use lobzique_core::work;
use lobzique_core::xml::{self, save_options::*};

const K: usize = 4;
const MAX_UNIT: usize = 512;
const RATIO: u64 = 3;
/// For the parser's state settling over the first copies.
const SLACK: u64 = 256;

struct Run {
    work: u64,
    depth: u64,
}

fuzz_target!(|data: &[u8]| {
    let Some((&flags, unit)) = data.split_first() else {
        return;
    };
    if unit.is_empty() || unit.len() > MAX_UNIT {
        return;
    }
    let runs = [K, 2 * K, 4 * K].map(|copies| run(&unit.repeat(copies), flags));
    let [Some(a), Some(b), Some(c)] = runs else {
        return;
    };
    let first = b.work.saturating_sub(a.work);
    let second = c.work.saturating_sub(b.work);
    // second / first <= RATIO * (c.depth + 1) / (b.depth + 1), without division
    let allowed = (RATIO * first + SLACK) * (c.depth.max(b.depth) + 1);
    assert!(
        second * (b.depth + 1) <= allowed,
        "work for {K}, {} and {} copies: {}, {}, {} (depths {}, {}, {})",
        2 * K,
        4 * K,
        a.work,
        b.work,
        c.work,
        a.depth,
        b.depth,
        c.depth,
    );
});

/// Parses and serializes `input`; `None` if it exceeds a parser limit.
fn run(input: &[u8], flags: u8) -> Option<Run> {
    let options = Options {
        max_attributes: Some(400),
        max_errors: Some(usize::from(flags & 1)),
        max_tree_depth: Some(400),
        parse_noscript_content_as_text: flags & 2 != 0,
    };
    work::take();
    let (document, root) = if flags & 4 == 0 {
        (parse_document(input, &options).ok()?.0, NodeId::DOCUMENT)
    } else {
        let mut document = Document::new(DocumentKind::Html);
        let fragment = document.create_fragment();
        let context = Context::new(ContextNamespace::Html, "body");
        parse_fragment(&mut document, fragment, input, &context, &options).ok()?;
        (document, fragment)
    };
    serialize(&document, root, flags & 8 != 0, flags & 2 != 0);
    _ = xml::write(&document, root, None, b"  ", AS_XHTML | FORMAT);
    let work = work::take();
    Some(Run {
        work,
        depth: depth(&document, root),
    })
}

/// How many levels of nodes are below `root`.
fn depth(document: &Document, root: NodeId) -> u64 {
    let mut deepest = 0;
    let mut levels = vec![(root, 0u64)];
    while let Some((id, level)) = levels.pop() {
        deepest = deepest.max(level);
        levels.extend(document.children(id).map(|child| (child, level + 1)));
    }
    deepest
}
