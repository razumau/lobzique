//! The XPath engine. Nokogiri's CSS compiles to XPath and libxml2 runs it,
//! so CSS semantics are XPath semantics: the CSS front end (`crate::css`)
//! builds the [`Expr`]s that Nokogiri's visitor would write as strings, and
//! [`evaluate`] runs them as libxml2 would. The XPath subset parser (plan.md
//! decision 2) will build the same trees.

mod ast;
mod eval;
#[cfg(test)]
mod tests;

pub use ast::{Arithmetic, Axis, Comparison, Expr, Function, NodeTest, Path, QName, Step};
pub use eval::{evaluate, Call, Outcome, Value};

use std::fmt;

/// An expression ready to evaluate: Nokogiri's XPath string, compiled.
#[derive(Debug)]
pub struct Query {
    pub(crate) expr: Expr,
    functions: Vec<String>,
}

impl Query {
    /// `functions` names each custom call, by its `site`.
    pub fn new(mut expr: Expr, functions: Vec<String>) -> Query {
        expr.merge_descendant_steps();
        Query { expr, functions }
    }

    pub fn function_name(&self, site: usize) -> &str {
        &self.functions[site]
    }
}

/// libxml2's XPath errors that Nokogiri raises as `XPath::SyntaxError`,
/// numbered as libxml2's `xmlXPathError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidExpression = 7,
    UnregisteredFunction = 9,
    UndefinedNamespacePrefix = 19,
    StackUsage = 23,
}

impl Error {
    /// The `xmlError` code: `XML_XPATH_EXPRESSION_OK` (1200) plus the
    /// XPath error number.
    pub fn code(self) -> i32 {
        1200 + self as i32
    }

    /// The message libxml2 gives it.
    pub fn message(self) -> &'static str {
        match self {
            Error::InvalidExpression => "Invalid expression",
            Error::UnregisteredFunction => "Unregistered function",
            Error::UndefinedNamespacePrefix => "Undefined namespace prefix",
            Error::StackUsage => "Stack usage error",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for Error {}
