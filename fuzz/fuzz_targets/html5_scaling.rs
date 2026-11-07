#![no_main]

//! Parsing and serializing must cost about the same per byte of input and
//! output however many copies of the input there are. `-timeout` catches
//! only catastrophic slowness: a quadratic path that takes 2s on 64 KB
//! passes it. So the input is repeated K, 2K and 4K times and the core
//! counts the work each run does (`lobzique_core::work`); per byte added
//! from 2K to 4K copies, the work may be at most RATIO times what it was
//! from K to 2K, where linear work keeps it at 1 and quadratic doubles it.
//!
//! The output need not grow with the copies at a steady rate: each copy
//! can reopen more of the formatting elements earlier copies left active,
//! with their attributes, until the Noah's Ark clause caps the list, so a
//! copy's output grows over the first copies. Hence bytes, not copies.
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
/// 3/2, as a numerator and denominator.
const RATIO: (u128, u128) = (3, 2);
/// For the parser's state settling over the first copies.
const SLACK: u64 = 256;

struct Run {
    work: u64,
    depth: u64,
    /// Bytes of input and of serialized HTML.
    size: u64,
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
    let delta = |from: &Run, to: &Run, field: fn(&Run) -> u64| {
        u128::from(field(to).saturating_sub(field(from)))
    };
    let size = |from: &Run, to: &Run| delta(from, to, |r| r.size).max(1);
    let (first, first_size) = (delta(&a, &b, |r| r.work), size(&a, &b));
    let (second, second_size) = (delta(&b, &c, |r| r.work), size(&b, &c));
    let (numerator, denominator) = RATIO;
    // second / second_size <= RATIO * first / first_size * (c.depth + 1) / (b.depth + 1),
    // without division
    let allowed = (numerator * first + denominator * u128::from(SLACK))
        * second_size
        * u128::from(c.depth.max(b.depth) + 1);
    assert!(
        denominator * second * first_size * u128::from(b.depth + 1) <= allowed,
        "work for {K}, {} and {} copies: {}, {}, {} (depths {}, {}, {}; sizes {}, {}, {})",
        2 * K,
        4 * K,
        a.work,
        b.work,
        c.work,
        a.depth,
        b.depth,
        c.depth,
        a.size,
        b.size,
        c.size,
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
    let html = serialize(&document, root, flags & 8 != 0, flags & 2 != 0);
    _ = xml::write(&document, root, None, b"  ", AS_XHTML | FORMAT);
    let work = work::take();
    Some(Run {
        work,
        depth: depth(&document, root),
        size: (input.len() + html.len()) as u64,
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
