#![no_main]

//! Serializing a fragment parsed in a body context and parsing the result
//! again must reach a fixed point: what a sanitizer outputs is parsed again
//! by a browser, and output that keeps changing is how mutation XSS gets
//! past a sanitizer. The HTML standard's serialization isn't a round trip
//! (a CR in text comes back as LF, nested <a> and <form> elements flatten),
//! so the first rounds may change the output; ROUNDS more may not. The
//! adoption agency algorithm gives up after 8 misnested levels, so a
//! formatting element reopened under deep misnesting moves down 8 levels
//! a round, as gumbo's does: ROUNDS grows with the depth limit.
//! <plaintext> never settles, by the standard's design: its end tag is
//! text, so each round adds one. Nor does an SVG or MathML <pre>,
//! <textarea> or <listing> that starts with a newline, by Nokogiri's:
//! `preserve_newline` doubles that newline as for HTML ones, but the parser
//! drops one only in HTML. Nor does a <script> whose text leaves the
//! tokenizer double-escaped (`<!--<script`), by the standard's: the
//! serialized `</script>` is text, so each round adds one. Inputs that parse
//! into any of these are skipped; a later round that creates one made markup
//! out of text.

use libfuzzer_sys::fuzz_target;
use lobzique_core::dom::{Document, DocumentKind, NodeId, NodeKind};
use lobzique_core::html5::{
    parse_fragment, prepend_newline, serialize, Context, ContextNamespace, Options,
};

const MAX_TREE_DEPTH: usize = 400;
const ROUNDS: usize = 4 + MAX_TREE_DEPTH / 8;
const HTML: &str = "http://www.w3.org/1999/xhtml";

#[derive(Debug)]
enum Skip {
    ParseLimit,
    NeverSettles,
}

fuzz_target!(|input: &[u8]| {
    let Ok(mut output) = round_trip(input) else {
        return;
    };
    for _ in 0..ROUNDS {
        let next = match round_trip(&output) {
            Ok(next) => next,
            Err(skip) => panic!(
                "{skip:?} after a round trip: {:?}",
                String::from_utf8_lossy(&output)
            ),
        };
        if next == output {
            return;
        }
        output = next;
    }
    panic!(
        "no fixed point after {ROUNDS} rounds: {:?}",
        String::from_utf8_lossy(&output)
    );
});

/// The fragment's serialization, with the newline after <pre>, <textarea>
/// and <listing> that the parser drops written back (`preserve_newline`).
fn round_trip(input: &[u8]) -> Result<Vec<u8>, Skip> {
    let (document, fragment) = parse(input).ok_or(Skip::ParseLimit)?;
    let never_settles = document
        .descendants(fragment)
        .any(|id| match document.node_kind(id) {
            NodeKind::Element(element) if &*element.name.ns == HTML => match &*element.name.local {
                "plaintext" => true,
                "script" => !closes(&document.content(id).unwrap_or_default()),
                _ => false,
            },
            NodeKind::Element(_) => prepend_newline(&document, id),
            _ => false,
        });
    if never_settles {
        return Err(Skip::NeverSettles);
    }
    Ok(serialize(&document, fragment, true, false))
}

fn parse(input: &[u8]) -> Option<(Document, NodeId)> {
    let options = Options {
        max_attributes: Some(400),
        max_errors: Some(0),
        max_tree_depth: Some(MAX_TREE_DEPTH),
        parse_noscript_content_as_text: false,
    };
    let context = Context::new(ContextNamespace::Html, "body");
    let mut document = Document::new(DocumentKind::Html);
    let fragment = document.create_fragment();
    parse_fragment(&mut document, fragment, input, &context, &options).ok()?;
    Some((document, fragment))
}

/// Whether the `</script>` the serializer writes after this script text
/// ends the script.
fn closes(script_text: &[u8]) -> bool {
    let html = [&b"<script>"[..], script_text, b"</script>"].concat();
    parse(&html).is_some_and(|(document, fragment)| {
        document
            .children(fragment)
            .next()
            .and_then(|script| document.content(script))
            .is_some_and(|text| text == script_text)
    })
}
