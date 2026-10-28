//! Evaluation as libxml2 does it: XPath 1.0's value types and comparisons,
//! node sets in document order, positions along each step's axis.
//!
//! Custom functions are Ruby methods, which can't run while the arena is
//! borrowed. [`evaluate`] answers each call it reaches from `answers`, and
//! when one is missing it notes the call, counts it as an empty node set and
//! carries on; the caller runs the noted calls with the borrow released and
//! evaluates again, until nothing is missing.

use super::ast::{Arithmetic, Axis, Comparison, Expr, Function, NodeTest, Path, QName, Step};
use super::{Error, Query};
use crate::dom::{Document, NodeId, NodeKind};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq)]
pub enum Value<'a> {
    /// In document order, without duplicates.
    NodeSet(Vec<NodeId>),
    Boolean(bool),
    Number(f64),
    String(Cow<'a, [u8]>),
}

impl Value<'_> {
    pub fn into_owned(self) -> Value<'static> {
        match self {
            Value::NodeSet(nodes) => Value::NodeSet(nodes),
            Value::Boolean(b) => Value::Boolean(b),
            Value::Number(n) => Value::Number(n),
            Value::String(s) => Value::String(Cow::Owned(s.into_owned())),
        }
    }
}

/// A custom function call the caller has to make.
#[derive(Debug, PartialEq)]
pub struct Call {
    pub site: usize,
    /// The context node, which the answer is for.
    pub node: NodeId,
    pub arguments: Vec<Value<'static>>,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Nodes(Vec<NodeId>),
    /// Calls to answer before evaluating again.
    Calls(Vec<Call>),
}

/// Evaluates `query` at `context`. `namespaces` binds prefixes, `xml`
/// always included; `answers` holds the custom calls made so far, by site
/// and context node.
pub fn evaluate(
    document: &Document,
    query: &Query,
    context: NodeId,
    namespaces: &HashMap<String, String>,
    answers: &HashMap<(usize, NodeId), Value<'static>>,
) -> Result<Outcome, Error> {
    // libxml2 resolves a prefix when it evaluates the step; resolving them
    // all first differs only when that step is never reached
    check_prefixes(&query.expr, namespaces)?;
    let evaluator = Evaluator {
        document,
        namespaces,
        answers,
        calls: RefCell::new(Vec::new()),
        noted: RefCell::new(HashSet::new()),
    };
    let value = evaluator.expr(
        &query.expr,
        Context {
            node: context,
            position: 1,
            size: 1,
        },
    );
    let calls = evaluator.calls.into_inner();
    if !calls.is_empty() {
        return Ok(Outcome::Calls(calls));
    }
    Ok(Outcome::Nodes(match value {
        Value::NodeSet(nodes) => nodes,
        _ => Vec::new(),
    }))
}

const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

fn check_prefixes(expr: &Expr, namespaces: &HashMap<String, String>) -> Result<(), Error> {
    let check_path = |path: &Path| -> Result<(), Error> {
        for step in &path.steps {
            if let NodeTest::Name(QName {
                prefix: Some(prefix),
                ..
            }) = &step.test
            {
                if prefix != "xml" && !namespaces.contains_key(prefix) {
                    return Err(Error::UndefinedNamespacePrefix);
                }
            }
            for predicate in &step.predicates {
                check_prefixes(predicate, namespaces)?;
            }
        }
        Ok(())
    };
    match expr {
        Expr::Or(list) | Expr::And(list) => list
            .iter()
            .try_for_each(|expr| check_prefixes(expr, namespaces)),
        Expr::Compare(_, left, right) | Expr::Arithmetic(_, left, right) => {
            check_prefixes(left, namespaces)?;
            check_prefixes(right, namespaces)
        }
        Expr::Call(_, arguments) | Expr::Custom { arguments, .. } => arguments
            .iter()
            .try_for_each(|expr| check_prefixes(expr, namespaces)),
        Expr::Path(path) => check_path(path),
        Expr::Union(paths) => paths.iter().try_for_each(check_path),
        Expr::Number(_) | Expr::Literal(_) => Ok(()),
    }
}

#[derive(Clone, Copy)]
struct Context {
    node: NodeId,
    position: usize,
    size: usize,
}

struct Evaluator<'a> {
    document: &'a Document,
    namespaces: &'a HashMap<String, String>,
    answers: &'a HashMap<(usize, NodeId), Value<'static>>,
    calls: RefCell<Vec<Call>>,
    noted: RefCell<HashSet<(usize, NodeId)>>,
}

impl<'a> Evaluator<'a> {
    fn expr<'q>(&self, expr: &'q Expr, context: Context) -> Value<'q> {
        match expr {
            Expr::Or(list) => Value::Boolean(list.iter().any(|e| self.boolean(e, context))),
            Expr::And(list) => Value::Boolean(list.iter().all(|e| self.boolean(e, context))),
            Expr::Compare(comparison, left, right) => {
                let (left, right) = (self.expr(left, context), self.expr(right, context));
                Value::Boolean(self.compare(*comparison, &left, &right))
            }
            Expr::Arithmetic(operator, left, right) => {
                let left = self.number(&self.expr(left, context));
                let right = self.number(&self.expr(right, context));
                Value::Number(match operator {
                    Arithmetic::Add => left + right,
                    Arithmetic::Subtract => left - right,
                    // C's fmod, as in libxml2
                    Arithmetic::Modulo => left % right,
                })
            }
            Expr::Number(n) => Value::Number(*n),
            Expr::Literal(s) => Value::String(Cow::Borrowed(s)),
            Expr::Path(path) => Value::NodeSet(self.path(path, context.node)),
            Expr::Union(paths) => {
                let mut nodes: Vec<NodeId> = paths
                    .iter()
                    .flat_map(|path| self.path(path, context.node))
                    .collect();
                if paths.len() > 1 {
                    self.document.sort_in_document_order(&mut nodes);
                }
                Value::NodeSet(nodes)
            }
            Expr::Call(function, arguments) => self.call(*function, arguments, context),
            Expr::Custom { site, arguments } => {
                if let Some(answer) = self.answers.get(&(*site, context.node)) {
                    return answer.clone();
                }
                if self.noted.borrow_mut().insert((*site, context.node)) {
                    let arguments = arguments
                        .iter()
                        .map(|argument| self.expr(argument, context).into_owned())
                        .collect();
                    self.calls.borrow_mut().push(Call {
                        site: *site,
                        node: context.node,
                        arguments,
                    });
                }
                Value::NodeSet(Vec::new())
            }
        }
    }

    fn call<'q>(&self, function: Function, arguments: &'q [Expr], context: Context) -> Value<'q> {
        let string = |i: usize| -> Cow<'q, [u8]> {
            arguments.get(i).map_or(Cow::Borrowed(&[][..]), |e| {
                self.string(self.expr(e, context))
            })
        };
        match function {
            Function::Position => Value::Number(context.position as f64),
            Function::Last => Value::Number(context.size as f64),
            Function::Count => match arguments.first().map(|e| self.expr(e, context)) {
                Some(Value::NodeSet(nodes)) => Value::Number(nodes.len() as f64),
                _ => Value::Number(f64::NAN),
            },
            Function::Contains => {
                let (haystack, needle) = (string(0), string(1));
                Value::Boolean(contains(&haystack, &needle))
            }
            Function::StartsWith => Value::Boolean(string(0).starts_with(&string(1))),
            Function::EndsWith => Value::Boolean(string(0).ends_with(&string(1))),
            Function::Concat => Value::String(Cow::Owned(
                (0..arguments.len())
                    .flat_map(|i| string(i).into_owned())
                    .collect(),
            )),
            Function::NormalizeSpace => {
                let string = string(0);
                let words: Vec<&[u8]> = string
                    .split(|&byte| is_blank(byte))
                    .filter(|word| !word.is_empty())
                    .collect();
                Value::String(Cow::Owned(words.join(&b' ')))
            }
            Function::Not => {
                Value::Boolean(!arguments.first().is_some_and(|e| self.boolean(e, context)))
            }
            Function::CssClass => Value::Boolean(css_class(&string(0), &string(1))),
        }
    }

    fn boolean(&self, expr: &Expr, context: Context) -> bool {
        if let Expr::Path(path) = expr {
            return self.exists(path, context.node);
        }
        match self.expr(expr, context) {
            Value::NodeSet(nodes) => !nodes.is_empty(),
            Value::Boolean(b) => b,
            Value::Number(n) => n != 0.0 && !n.is_nan(),
            Value::String(s) => !s.is_empty(),
        }
    }

    fn string<'q>(&self, value: Value<'q>) -> Cow<'q, [u8]> {
        match value {
            Value::NodeSet(nodes) => Cow::Owned(
                nodes
                    .first()
                    .map(|&node| self.string_value(node))
                    .unwrap_or_default(),
            ),
            Value::Boolean(b) => Cow::Borrowed(if b { b"true" } else { b"false" }),
            Value::Number(n) => Cow::Owned(number_to_string(n).into_bytes()),
            Value::String(s) => s,
        }
    }

    fn number(&self, value: &Value<'_>) -> f64 {
        match value {
            Value::NodeSet(nodes) => nodes
                .first()
                .map_or(f64::NAN, |&node| string_to_number(&self.string_value(node))),
            Value::Boolean(b) => f64::from(u8::from(*b)),
            Value::Number(n) => *n,
            Value::String(s) => string_to_number(s),
        }
    }

    /// libxml2's `xmlXPathCastNodeToString`, which is `xmlNodeGetContent`.
    fn string_value(&self, node: NodeId) -> Vec<u8> {
        self.document.content(node).unwrap_or_default()
    }

    /// XPath 1.0's comparison rules: a node set compares as each of its
    /// nodes in turn.
    fn compare(&self, comparison: Comparison, left: &Value<'_>, right: &Value<'_>) -> bool {
        use Value::{Boolean, NodeSet, Number, String};
        let equality = matches!(comparison, Comparison::Equal | Comparison::NotEqual);
        match (left, right) {
            (NodeSet(a), NodeSet(b)) => {
                let b: Vec<Vec<u8>> = b.iter().map(|&node| self.string_value(node)).collect();
                a.iter().any(|&node| {
                    let a = self.string_value(node);
                    b.iter().any(|b| {
                        if equality {
                            compare_strings(comparison, &a, b)
                        } else {
                            compare_numbers(comparison, string_to_number(&a), string_to_number(b))
                        }
                    })
                })
            }
            (NodeSet(nodes), Boolean(_)) | (Boolean(_), NodeSet(nodes)) => {
                let set = Boolean(!nodes.is_empty());
                let (left, right) = if matches!(left, NodeSet(_)) {
                    (&set, right)
                } else {
                    (left, &set)
                };
                self.compare(comparison, left, right)
            }
            (NodeSet(nodes), other) => nodes.iter().any(|&node| {
                let value = String(Cow::Owned(self.string_value(node)));
                self.compare(comparison, &value, other)
            }),
            (other, NodeSet(nodes)) => nodes.iter().any(|&node| {
                let value = String(Cow::Owned(self.string_value(node)));
                self.compare(comparison, other, &value)
            }),
            _ if !equality => compare_numbers(comparison, self.number(left), self.number(right)),
            (Boolean(_), _) | (_, Boolean(_)) => {
                let (a, b) = (self.truthy(left), self.truthy(right));
                (a == b) == (comparison == Comparison::Equal)
            }
            (Number(_), _) | (_, Number(_)) => {
                compare_numbers(comparison, self.number(left), self.number(right))
            }
            (String(a), String(b)) => compare_strings(comparison, a, b),
        }
    }

    fn truthy(&self, value: &Value<'_>) -> bool {
        match value {
            Value::NodeSet(nodes) => !nodes.is_empty(),
            Value::Boolean(b) => *b,
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::String(s) => !s.is_empty(),
        }
    }

    fn path(&self, path: &Path, context: NodeId) -> Vec<NodeId> {
        let mut nodes = vec![if path.absolute {
            NodeId::DOCUMENT
        } else {
            context
        }];
        for step in &path.steps {
            if nodes.is_empty() {
                break;
            }
            nodes = self.step(step, &nodes);
        }
        nodes
    }

    /// Whether `path` finds a node from `context`. The last step stops at
    /// the first node that passes, unless its predicates read positions,
    /// which `:has(…)` and `[@attr]` don't.
    fn exists(&self, path: &Path, context: NodeId) -> bool {
        let Some((last, init)) = path.steps.split_last() else {
            return true;
        };
        if last.predicates.iter().any(Expr::is_positional) {
            return !self.path(path, context).is_empty();
        }
        let mut nodes = vec![if path.absolute {
            NodeId::DOCUMENT
        } else {
            context
        }];
        for step in init {
            if nodes.is_empty() {
                return false;
            }
            nodes = self.step(step, &nodes);
        }
        nodes.into_iter().any(|node| {
            self.axis(last.axis, node).any(|candidate| {
                self.passes(&last.test, last.axis, candidate)
                    && last.predicates.iter().all(|predicate| {
                        let context = Context {
                            node: candidate,
                            position: 1,
                            size: 1,
                        };
                        self.boolean(predicate, context)
                    })
            })
        })
    }

    /// One step from each node of `input`, which is in document order.
    fn step(&self, step: &Step, input: &[NodeId]) -> Vec<NodeId> {
        let document = self.document;
        // Descendants of a node already searched are found again from it,
        // unless positions differ by starting point
        let subsumes = matches!(step.axis, Axis::Descendant | Axis::DescendantOrSelf)
            && !step.predicates.iter().any(Expr::is_positional)
            && !input.iter().any(|&node| self.is_attribute(node));
        // `[k]` keeps the k-th candidate at most, so the axis is walked no
        // further, as libxml2 does for `following-sibling::*[1]` (`a + b`)
        let limit = match step.predicates.first() {
            Some(&Expr::Number(k)) if k >= 1.0 && k.fract() == 0.0 => k as usize,
            Some(Expr::Number(_)) => 0,
            _ => usize::MAX,
        };
        let mut output = Vec::new();
        let mut searched: Option<NodeId> = None;
        for &node in input {
            if subsumes {
                if searched.is_some_and(|root| document.ancestors(node).any(|a| a == root)) {
                    continue;
                }
                searched = Some(node);
            }
            let mut candidates: Vec<NodeId> = self
                .axis(step.axis, node)
                .filter(|&candidate| self.passes(&step.test, step.axis, candidate))
                .take(limit)
                .collect();
            for predicate in &step.predicates {
                candidates = self.filter(candidates, predicate);
            }
            if step.axis.is_reverse() {
                candidates.reverse();
            }
            output.extend(candidates);
        }
        if input.len() > 1 && !subsumes {
            document.sort_in_document_order(&mut output);
        }
        output
    }

    fn filter(&self, candidates: Vec<NodeId>, predicate: &Expr) -> Vec<NodeId> {
        let size = candidates.len();
        candidates
            .into_iter()
            .enumerate()
            .filter(|&(i, node)| {
                let context = Context {
                    node,
                    position: i + 1,
                    size,
                };
                if let Expr::Path(path) = predicate {
                    return self.exists(path, node);
                }
                match self.expr(predicate, context) {
                    Value::Number(n) => n == context.position as f64,
                    value => self.truthy(&value),
                }
            })
            .map(|(_, node)| node)
            .collect()
    }

    /// The nodes on `axis` from `node`, in the axis's order.
    fn axis(&self, axis: Axis, node: NodeId) -> Box<dyn Iterator<Item = NodeId> + 'a> {
        let document = self.document;
        let not_doctype = |&id: &NodeId| !matches!(document.node_kind(id), NodeKind::Doctype(_));
        let is_attribute = self.is_attribute(node);
        let siblings = |next: fn(&Document, NodeId) -> Option<NodeId>| {
            std::iter::successors(next(document, node), move |&s| next(document, s))
        };
        match axis {
            Axis::Child | Axis::Descendant | Axis::FollowingSibling | Axis::PrecedingSibling
                if is_attribute =>
            {
                Box::new(std::iter::empty())
            }
            Axis::Child => Box::new(document.children(node)),
            // libxml2 walks past doctypes here, though `child::` has them
            Axis::Descendant => Box::new(document.descendants(node).filter(not_doctype)),
            Axis::DescendantOrSelf if is_attribute => Box::new(std::iter::once(node)),
            Axis::DescendantOrSelf => Box::new(
                std::iter::once(node).chain(document.descendants(node).filter(not_doctype)),
            ),
            Axis::Self_ => Box::new(std::iter::once(node)),
            Axis::Parent => Box::new(document.parent(node).into_iter()),
            Axis::Ancestor => Box::new(document.ancestors(node)),
            Axis::AncestorOrSelf => Box::new(std::iter::once(node).chain(document.ancestors(node))),
            Axis::Attribute => Box::new(document.attributes(node).iter().copied()),
            Axis::FollowingSibling => Box::new(siblings(Document::next_sibling)),
            Axis::PrecedingSibling => Box::new(siblings(Document::previous_sibling)),
            Axis::Following => {
                let mut nodes = Vec::new();
                let start = match (is_attribute, document.parent(node)) {
                    (true, Some(element)) => {
                        nodes.extend(document.descendants(element));
                        element
                    }
                    _ => node,
                };
                for ancestor in std::iter::once(start).chain(document.ancestors(start)) {
                    let mut sibling = document.next_sibling(ancestor);
                    while let Some(s) = sibling {
                        nodes.push(s);
                        nodes.extend(document.descendants(s));
                        sibling = document.next_sibling(s);
                    }
                }
                Box::new(nodes.into_iter())
            }
            Axis::Preceding => {
                let mut nodes = Vec::new();
                let start = match (is_attribute, document.parent(node)) {
                    (true, Some(element)) => element,
                    _ => node,
                };
                for ancestor in std::iter::once(start).chain(document.ancestors(start)) {
                    let mut sibling = document.previous_sibling(ancestor);
                    while let Some(s) = sibling {
                        let mut subtree: Vec<NodeId> = document.descendants(s).collect();
                        subtree.reverse();
                        nodes.extend(subtree);
                        nodes.push(s);
                        sibling = document.previous_sibling(s);
                    }
                }
                Box::new(nodes.into_iter())
            }
        }
    }

    fn is_attribute(&self, node: NodeId) -> bool {
        matches!(self.document.node_kind(node), NodeKind::Attribute(_))
    }

    fn passes(&self, test: &NodeTest, axis: Axis, node: NodeId) -> bool {
        let kind = self.document.node_kind(node);
        let principal = match axis {
            Axis::Attribute => matches!(kind, NodeKind::Attribute(_)),
            _ => matches!(kind, NodeKind::Element(_)),
        };
        match test {
            NodeTest::Node => true,
            NodeTest::Text => matches!(kind, NodeKind::Text(_) | NodeKind::CData(_)),
            NodeTest::Comment => matches!(kind, NodeKind::Comment(_)),
            NodeTest::Any => principal,
            NodeTest::LocalName(local) => principal && self.local_name(node) == local.as_str(),
            NodeTest::Name(name) => {
                principal
                    && self.local_name(node) == name.local
                    && self.document.namespace(node).map(|ns| ns.href)
                        == name.prefix.as_deref().map(|prefix| self.resolve(prefix))
            }
        }
    }

    fn local_name(&self, node: NodeId) -> &str {
        match self.document.node_kind(node) {
            NodeKind::Element(element) => &element.name.local,
            NodeKind::Attribute(attribute) => &attribute.name.local,
            _ => "",
        }
    }

    fn resolve(&self, prefix: &str) -> &str {
        match self.namespaces.get(prefix) {
            Some(href) => href,
            None if prefix == "xml" => XML_NAMESPACE,
            None => unreachable!("prefixes are checked before evaluation"),
        }
    }
}

/// libxml2's `IS_BLANK_CH`
fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// Nokogiri's `css-class` builtin: `needle` starts a word of `haystack` and
/// ends at a blank or the end. An empty needle is always there.
fn css_class(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    let mut rest = haystack;
    while !rest.is_empty() {
        if rest.starts_with(needle) && rest.get(needle.len()).is_none_or(|&b| is_blank(b)) {
            return true;
        }
        let word = rest.iter().position(|&b| is_blank(b)).unwrap_or(rest.len());
        rest = &rest[word..];
        let blanks = rest
            .iter()
            .position(|&b| !is_blank(b))
            .unwrap_or(rest.len());
        rest = &rest[blanks..];
    }
    false
}

fn compare_strings(comparison: Comparison, a: &[u8], b: &[u8]) -> bool {
    match comparison {
        Comparison::Equal => a == b,
        Comparison::NotEqual => a != b,
        _ => compare_numbers(comparison, string_to_number(a), string_to_number(b)),
    }
}

fn compare_numbers(comparison: Comparison, a: f64, b: f64) -> bool {
    match comparison {
        Comparison::Equal => a == b,
        Comparison::NotEqual => a != b,
        Comparison::Less => a < b,
        Comparison::LessOrEqual => a <= b,
        Comparison::Greater => a > b,
        Comparison::GreaterOrEqual => a >= b,
    }
}

/// libxml2's `xmlXPathStringEvalNumber`: blanks, an optional minus, digits
/// with an optional fraction and exponent, blanks; anything else is NaN.
pub fn string_to_number(s: &[u8]) -> f64 {
    let start = s.iter().position(|&b| !is_blank(b)).unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|&b| !is_blank(b))
        .map_or(start, |i| i + 1);
    let s = &s[start..end.max(start)];
    let unsigned = s.strip_prefix(b"-").unwrap_or(s);
    let mantissa = unsigned
        .iter()
        .position(|&b| matches!(b, b'e' | b'E'))
        .unwrap_or(unsigned.len());
    let (digits, exponent) = unsigned.split_at(mantissa);
    let has_digit = digits.iter().any(u8::is_ascii_digit);
    let valid_mantissa = digits.iter().all(|&b| b.is_ascii_digit() || b == b'.')
        && digits.iter().filter(|&&b| b == b'.').count() <= 1;
    let valid_exponent = exponent.is_empty() || {
        let e = &exponent[1..];
        let e = e
            .strip_prefix(b"+")
            .or_else(|| e.strip_prefix(b"-"))
            .unwrap_or(e);
        !e.is_empty() && e.iter().all(u8::is_ascii_digit)
    };
    if !has_digit || !valid_mantissa || !valid_exponent {
        return f64::NAN;
    }
    std::str::from_utf8(s)
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(f64::NAN)
}

/// XPath's `string()` of a number: integers without a fraction.
pub fn number_to_string(n: f64) -> String {
    if n.is_nan() {
        "NaN".into()
    } else if n.is_infinite() {
        if n > 0.0 { "Infinity" } else { "-Infinity" }.into()
    } else if n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}
