//! The XPath engine. Nokogiri's CSS compiles to XPath and libxml2 runs it,
//! so CSS semantics are XPath semantics: the CSS front end (`crate::css`)
//! builds the [`Expr`]s that Nokogiri's visitor would write as strings, and
//! [`evaluate`] runs them as libxml2 would. [`parse`] builds the same trees
//! from XPath strings (plan.md decision 2).

mod ast;
pub(crate) mod chars;
mod eval;
mod parse;
#[cfg(test)]
mod tests;

use ast::Merge;
pub use ast::{
    Arithmetic, Axis, Comparison, Expr, Filter, Function, NodeTest, Operator, Path, QName, Step,
};
pub use eval::{evaluate, Answer, Call, Outcome, Value};
pub use parse::{parse, ParseError, MAX_NESTING};

use std::fmt;

/// Nokogiri's namespace for the functions of custom handlers.
pub const NOKOGIRI_URI: &str = "http://www.nokogiri.org/default_ns/ruby/extensions_functions";
/// Nokogiri's namespace for the functions it defines itself.
pub const NOKOGIRI_BUILTIN_URI: &str = "https://www.nokogiri.org/default_ns/ruby/builtins";
/// The namespace of libxml2's `escape-uri`.
pub const XQUERY_FUNCTIONS_URI: &str = "http://www.w3.org/2002/08/xquery-functions";

/// An expression ready to evaluate: Nokogiri's XPath string, compiled.
#[derive(Debug)]
pub struct Query {
    pub(crate) expr: Expr,
    functions: Vec<FunctionName>,
}

/// The name a call to a function outside XPath 1.0 gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionName {
    pub prefix: Option<String>,
    pub local: String,
}

impl Query {
    /// A query compiled from CSS. `functions` names each custom call, by
    /// its `site`, as Nokogiri's visitor writes `nokogiri:name(…)`.
    pub fn new(mut expr: Expr, functions: Vec<String>) -> Query {
        expr.merge_descendant_steps(Merge::UnlessPositional);
        let functions = functions
            .into_iter()
            .map(|local| FunctionName {
                prefix: Some("nokogiri".into()),
                local,
            })
            .collect();
        Query { expr, functions }
    }

    /// A query parsed from an XPath string.
    fn xpath(mut expr: Expr, functions: Vec<FunctionName>) -> Query {
        expr.merge_descendant_steps(Merge::WithoutPredicates);
        Query { expr, functions }
    }

    pub fn function(&self, site: usize) -> &FunctionName {
        &self.functions[site]
    }
}

/// libxml2's XPath errors that Nokogiri raises as `XPath::SyntaxError`,
/// numbered as libxml2's `xmlXPathError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    UnfinishedLiteral = 2,
    StartLiteral = 3,
    VariableReference = 4,
    UndefinedVariable = 5,
    InvalidPredicate = 6,
    InvalidExpression = 7,
    Unclosed = 8,
    UnregisteredFunction = 9,
    InvalidType = 11,
    InvalidArity = 12,
    UndefinedNamespacePrefix = 19,
    Encoding = 20,
    InvalidChar = 21,
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
            Error::UnfinishedLiteral => "Unfinished literal",
            Error::StartLiteral => "Start of literal",
            Error::VariableReference => "Expected $ for variable reference",
            Error::UndefinedVariable => "Undefined variable",
            Error::InvalidPredicate => "Invalid predicate",
            Error::InvalidExpression => "Invalid expression",
            // sic: libxml2's message for a missing `)`
            Error::Unclosed => "Missing closing curly brace",
            Error::UnregisteredFunction => "Unregistered function",
            Error::InvalidType => "Invalid type",
            Error::InvalidArity => "Invalid number of arguments",
            Error::UndefinedNamespacePrefix => "Undefined namespace prefix",
            Error::Encoding => "Encoding error",
            Error::InvalidChar => "Char out of XML range",
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
