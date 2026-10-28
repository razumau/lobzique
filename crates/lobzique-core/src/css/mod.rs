//! Nokogiri's CSS dialect, compiled into the XPath that Nokogiri's visitor
//! would write for it (ARCHITECTURE.md). The tokenizer, the parser's tables
//! and the visitor are ported from Nokogiri, so a selector selects what it
//! selects there, quirks and all: `a:first` is the first `a` among its
//! siblings, `node.css("div p")` finds only `div`s inside `node`, and
//! `[x^=""]` matches elements without `x`.

mod parser;
mod tables;
#[cfg(test)]
mod tests;
mod tokenizer;
mod translate;

use crate::xpath::{self, Expr, Query};
use std::fmt;
use translate::{Failure, Translator};

/// A CSS syntax error, Nokogiri's `CSS::SyntaxError`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError(pub String);

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SyntaxError {}

/// The XPath prefixes Nokogiri puts before a selector, the implied
/// contexts of each searchable class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prefix {
    /// `//`, from a document
    Global,
    /// `.//`, from a node
    Subtree,
    /// `./`, for `Searchable#>`
    Current,
    /// `/`
    Root,
    /// `self::`, which node sets and fragments add to `.//`
    Self_,
}

impl Prefix {
    /// The prefix Nokogiri writes as `prefix`.
    pub fn parse(prefix: &str) -> Option<Prefix> {
        Some(match prefix {
            "//" => Prefix::Global,
            ".//" => Prefix::Subtree,
            "./" => Prefix::Current,
            "/" => Prefix::Root,
            "self::" => Prefix::Self_,
            _ => return None,
        })
    }
}

/// What Nokogiri's `XPathVisitor` is configured with.
#[derive(Clone, Debug)]
pub struct Options {
    /// Names without a prefix match in any namespace, as for HTML5
    /// documents.
    pub html: bool,
    /// Nokogiri's builtin `css-class` for classes.
    pub builtins: bool,
    /// The namespace bindings have an `xmlns` key, a default namespace for
    /// names without a prefix (outside HTML).
    pub default_namespace: bool,
    pub prefixes: Vec<Prefix>,
    /// Only the first selector of a list, as `Searchable#>` takes it. The
    /// others are still translated, so their syntax errors are raised.
    pub first_only: bool,
}

/// A compiled selector list. `query` holds the error libxml2 would raise
/// for the XPath Nokogiri builds, which surfaces only when it is evaluated.
#[derive(Debug)]
pub struct Selector {
    pub query: Result<Query, xpath::Error>,
}

/// `CSS.xpath_for` with the visitor `options` describe, for every implied
/// prefix, joined with `|`.
pub fn compile(rule: &str, options: &Options) -> Result<Selector, SyntaxError> {
    if rule.is_empty() {
        return Err(SyntaxError("empty CSS selector".into()));
    }
    let selectors = parser::parse(rule)?;
    let mut translator = Translator::new(options);
    let mut paths = Vec::new();
    let mut invalid = false;
    for prefix in &options.prefixes {
        for (index, selector) in selectors.iter().enumerate() {
            // `.first` comes after the visitor has translated every selector
            let kept = index == 0 || !options.first_only;
            match translator.selector(selector, *prefix) {
                Ok(path) if kept => paths.push(path),
                Err(Failure::Syntax(error)) => return Err(error),
                Err(Failure::Invalid) if kept => invalid = true,
                Ok(_) | Err(Failure::Invalid) => {}
            }
        }
    }
    let query = if invalid {
        Err(xpath::Error::InvalidExpression)
    } else {
        Ok(Query::new(Expr::Union(paths), translator.functions))
    };
    Ok(Selector { query })
}
