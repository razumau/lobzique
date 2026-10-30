//! HTML parsing and serialization as Nokogiri::HTML5 does them, over the
//! arena DOM.

mod parse;
mod serialize;
#[cfg(test)]
mod tests;

pub use parse::{
    parse_document, parse_fragment, parse_in_context, quirks_mode_for_doctype, Context,
    ContextNamespace, LimitError, Options, ParseError, Parsed, QuirksMode,
};
pub use serialize::{prepend_newline, serialize};

use crate::dom::QualName;
use html5ever::ns;

/// Elements with no end tag, which the parser never leaves open.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "img", "input",
    "keygen", "link", "meta", "param", "source", "track", "wbr",
];

/// Whether an element counts as HTML for Nokogiri, which gives HTML
/// elements no namespace: in the HTML namespace or in none.
fn is_html(name: &QualName) -> bool {
    name.ns == ns!(html) || name.ns == ns!()
}
