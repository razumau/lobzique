#![no_main]

//! Catches superlinear work that is too small for `-timeout` to notice.
//! The input is repeated K, 2K and 4K times, and the core counts the work
//! of each run (`lobzique_core::work`). Per byte of input and output,
//! the second step can cost at most RATIO times the first. Linear work keeps
//! the ratio at 1. Quadratic work doubles it.
//!
//! - Bytes, not copies: the output of a copy varies, for example while reopened
//!   formatting elements accumulate up to the Noah's Ark cap.
//! - K = 12: a token that spans copies (a quoted attribute value) makes work
//!   arrive in steps. The samples split the steps evenly only if K
//!   is a multiple of the period, typically 2, 3, 4 or 6.
//! - Each insertion walks up the tree, so the allowance grows with depth.
//!
//! The first byte picks the options, as in the html5 target.

use libfuzzer_sys::fuzz_target;
use lobzique_core::dom::{Document, DocumentKind, NodeId};
use lobzique_core::html5::{
    parse_document, parse_fragment, serialize, Context, ContextNamespace, Options,
};
use lobzique_core::work;
use lobzique_core::xml::{self, save_options::*};

const K: usize = 12;
const MAX_UNIT: usize = 512;
const RATIO: (u128, u128) = (3, 2);
/// For the state of the parser to settle over the first copies.
const SLACK: u64 = 256;

struct Run {
    work: u64,
    depth: u64,
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
    // second / second_size <= RATIO * first / first_size * (c.depth + 1)
    // / (b.depth + 1), written without division.
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

/// Parses and serializes `input`. `None` if it exceeds a parser limit.
fn run(input: &[u8], flags: u8) -> Option<Run> {
    let options = Options {
        max_attributes: Some(400),
        max_errors: Some(usize::from(flags & 1)),
        max_tree_depth: Some(400),
        parse_noscript_content_as_text: flags & 2 != 0,
        keep_bom: false,
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

fn depth(document: &Document, root: NodeId) -> u64 {
    let mut deepest = 0;
    let mut levels = vec![(root, 0u64)];
    while let Some((id, level)) = levels.pop() {
        deepest = deepest.max(level);
        levels.extend(document.children(id).map(|child| (child, level + 1)));
    }
    deepest
}
