//! Evaluation as libxml2 does it: XPath 1.0's value types and comparisons,
//! node sets in document order, positions along each step's axis, and
//! errors where libxml2 raises them.
//!
//! Custom functions are Ruby methods, which can't run while the arena is
//! borrowed. [`evaluate`] answers each call it reaches from `answers`, and
//! when one is missing it notes the call, counts it as an empty node set and
//! carries on; the caller runs the noted calls with the borrow released and
//! evaluates again, until nothing is missing. An answer may be the error
//! the call raises, such as a function the handler doesn't have: raised
//! only once an evaluation with every answer reaches it, since the empty
//! node sets standing in can lead to calls libxml2 never makes.

use super::ast::{
    Arithmetic, Axis, Comparison, Expr, Filter, Function, NodeTest, Operator, Path, QName, Step,
};
use super::chars::is_blank;
use super::{Error, Query, NOKOGIRI_BUILTIN_URI, NOKOGIRI_URI};
use crate::dom::{Document, DocumentKind, NodeId, NodeKind};
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

/// What a custom call returned, or the error it raises.
pub type Answer = Result<Value<'static>, Error>;

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
    Value(Value<'static>),
    /// Calls to answer before evaluating again.
    Calls(Vec<Call>),
}

/// Evaluates `query` at `context`. `namespaces` binds prefixes beyond the
/// ones Nokogiri always binds, and `variables` names, as strings;
/// `answers` holds the custom calls made so far, by site and context node.
pub fn evaluate(
    document: &Document,
    query: &Query,
    context: NodeId,
    namespaces: &HashMap<String, String>,
    variables: &HashMap<String, Vec<u8>>,
    answers: &HashMap<(usize, NodeId), Answer>,
) -> Result<Outcome, Error> {
    // libxml2 resolves a prefix when it evaluates the step; resolving them
    // all first differs only when that step is never reached
    check_prefixes(&query.expr, namespaces)?;
    let evaluator = Evaluator {
        document,
        query,
        namespaces,
        variables,
        answers,
        calls: RefCell::new(Vec::new()),
        noted: RefCell::new(HashSet::new()),
        root: document.ancestors(context).last().unwrap_or(context),
        ids: RefCell::new(None),
    };
    let value = evaluator.expr(
        &query.expr,
        Context {
            node: context,
            position: 1,
            size: 1,
        },
    );
    // libxml2 makes the calls it reaches before an error
    let calls = evaluator.calls.into_inner();
    if !calls.is_empty() {
        return Ok(Outcome::Calls(calls));
    }
    Ok(Outcome::Value(value?.into_owned()))
}

const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

/// The namespace `prefix` names: one the caller binds, or one libxml2 or
/// Nokogiri binds in every context.
fn lookup<'n>(namespaces: &'n HashMap<String, String>, prefix: &str) -> Option<&'n str> {
    if let Some(href) = namespaces.get(prefix) {
        return Some(href);
    }
    match prefix {
        "xml" => Some(XML_NAMESPACE),
        "nokogiri" => Some(NOKOGIRI_URI),
        "nokogiri-builtin" => Some(NOKOGIRI_BUILTIN_URI),
        _ => None,
    }
}

fn check_prefixes(expr: &Expr, namespaces: &HashMap<String, String>) -> Result<(), Error> {
    let mut result = Ok(());
    expr.for_each_step(&mut |step| {
        let prefix = match &step.test {
            NodeTest::Name(QName {
                prefix: Some(prefix),
                ..
            })
            | NodeTest::AnyIn(prefix) => prefix,
            _ => return,
        };
        if lookup(namespaces, prefix).is_none() {
            result = Err(Error::UndefinedNamespacePrefix);
        }
    });
    result
}

#[derive(Clone, Copy)]
struct Context {
    node: NodeId,
    position: usize,
    size: usize,
}

type Eval<T> = Result<T, Error>;

struct Evaluator<'a> {
    document: &'a Document,
    query: &'a Query,
    namespaces: &'a HashMap<String, String>,
    variables: &'a HashMap<String, Vec<u8>>,
    answers: &'a HashMap<(usize, NodeId), Answer>,
    calls: RefCell<Vec<Call>>,
    noted: RefCell<HashSet<(usize, NodeId)>>,
    /// The root of the context node's tree, which `id()` searches after the
    /// document.
    root: NodeId,
    /// The element each ID names, built when `id()` first needs it.
    ids: RefCell<Option<HashMap<Vec<u8>, NodeId>>>,
}

impl<'a> Evaluator<'a> {
    fn expr<'q>(&self, expr: &'q Expr, context: Context) -> Eval<Value<'q>> {
        Ok(match expr {
            Expr::Or(list) => {
                for expr in list {
                    if self.boolean(expr, context)? {
                        return Ok(Value::Boolean(true));
                    }
                }
                Value::Boolean(false)
            }
            Expr::And(list) => {
                for expr in list {
                    if !self.boolean(expr, context)? {
                        return Ok(Value::Boolean(false));
                    }
                }
                Value::Boolean(true)
            }
            Expr::Operation(first, rest) => {
                let mut value = self.expr(first, context)?;
                for (operator, operand) in rest {
                    let right = self.expr(operand, context)?;
                    value = match *operator {
                        Operator::Compare(comparison) => {
                            Value::Boolean(self.compare(comparison, &value, &right))
                        }
                        Operator::Arithmetic(operator) => {
                            let (left, right) = (self.number(&value), self.number(&right));
                            Value::Number(match operator {
                                Arithmetic::Add => left + right,
                                Arithmetic::Subtract => left - right,
                                Arithmetic::Multiply => left * right,
                                Arithmetic::Divide => left / right,
                                // C's fmod, as in libxml2
                                Arithmetic::Modulo => left % right,
                            })
                        }
                    };
                }
                value
            }
            Expr::Negate(expr) => Value::Number(-self.number(&self.expr(expr, context)?)),
            Expr::Number(n) => Value::Number(*n),
            Expr::Literal(s) => Value::String(Cow::Borrowed(s)),
            Expr::Variable(name) => self.variable(name)?,
            Expr::Path(path) => Value::NodeSet(self.path(path, context.node)?),
            Expr::Filter(filter) => Value::NodeSet(self.filter_expr(filter, context)?),
            Expr::Union(list) => self.union(list, context)?,
            Expr::Call(function, arguments) => self.call(*function, arguments, context)?,
            Expr::Custom { site, arguments } => self.custom(*site, arguments, context)?,
        })
    }

    fn variable(&self, name: &QName) -> Eval<Value<'static>> {
        // Nokogiri binds variables without a namespace
        if let Some(prefix) = &name.prefix {
            self.resolve(prefix)?;
            return Err(Error::UndefinedVariable);
        }
        let value = self
            .variables
            .get(&name.local)
            .ok_or(Error::UndefinedVariable)?;
        Ok(Value::String(Cow::Owned(value.clone())))
    }

    fn resolve(&self, prefix: &str) -> Eval<&'a str> {
        lookup(self.namespaces, prefix).ok_or(Error::UndefinedNamespacePrefix)
    }

    /// libxml2 merges each operand into the ones before it, checking that
    /// both are node sets once it has evaluated the second.
    fn union<'q>(&self, list: &'q [Expr], context: Context) -> Eval<Value<'q>> {
        let Some((first, rest)) = list.split_first() else {
            return Ok(Value::NodeSet(Vec::new()));
        };
        let mut value = self.expr(first, context)?;
        for expr in rest {
            let next = self.expr(expr, context)?;
            let (Value::NodeSet(mut nodes), Value::NodeSet(more)) = (value, next) else {
                return Err(Error::InvalidType);
            };
            nodes.extend(more);
            value = Value::NodeSet(nodes);
        }
        if let Value::NodeSet(nodes) = &mut value {
            if !rest.is_empty() {
                self.document.sort_in_document_order(nodes);
            }
        }
        Ok(value)
    }

    fn filter_expr(&self, filter: &Filter, context: Context) -> Eval<Vec<NodeId>> {
        let Value::NodeSet(mut nodes) = self.expr(&filter.primary, context)? else {
            return Err(Error::InvalidType);
        };
        for predicate in &filter.predicates {
            nodes = self.filter(nodes, predicate)?;
        }
        for step in &filter.steps {
            if nodes.is_empty() {
                break;
            }
            nodes = self.step(step, &nodes)?;
        }
        Ok(nodes)
    }

    /// libxml2 evaluates the arguments, then checks how many there are,
    /// then their types.
    fn call<'q>(
        &self,
        function: Function,
        arguments: &'q [Expr],
        context: Context,
    ) -> Eval<Value<'q>> {
        // whether a path finds a node needs only the first
        if let (Function::Not, [Expr::Path(path)]) = (function, arguments) {
            return Ok(Value::Boolean(!self.exists(path, context.node)?));
        }
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            values.push(self.expr(argument, context)?);
        }
        self.apply(function, values, context)
    }

    fn custom<'q>(&self, site: usize, arguments: &'q [Expr], context: Context) -> Eval<Value<'q>> {
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            values.push(self.expr(argument, context)?);
        }
        let name = self.query.function(site);
        if let Some(prefix) = &name.prefix {
            let href = self.resolve(prefix)?;
            if let Some(function) = Function::in_namespace(href, &name.local) {
                return self.apply(function, values, context);
            }
        }
        if let Some(answer) = self.answers.get(&(site, context.node)) {
            return answer.clone();
        }
        if self.noted.borrow_mut().insert((site, context.node)) {
            self.calls.borrow_mut().push(Call {
                site,
                node: context.node,
                arguments: values.into_iter().map(Value::into_owned).collect(),
            });
        }
        Ok(Value::NodeSet(Vec::new()))
    }

    fn apply<'q>(
        &self,
        function: Function,
        mut values: Vec<Value<'q>>,
        context: Context,
    ) -> Eval<Value<'q>> {
        use Function::*;
        let count = values.len();
        let arity = match function {
            Last | Position | True | False => count == 0,
            LocalName | NamespaceUri | Name | String | StringLength | NormalizeSpace | Number => {
                count <= 1
            }
            Count | Id | Boolean | Not | Lang | Sum | Floor | Ceiling | Round | LocalNameIs => {
                count == 1
            }
            StartsWith | Contains | SubstringBefore | SubstringAfter | EndsWith | CssClass
            | EscapeUri => count == 2,
            Substring => count == 2 || count == 3,
            Concat => count >= 2,
            Translate => count == 3,
        };
        if !arity {
            return Err(Error::InvalidArity);
        }
        // The context node stands in for a missing argument
        let node_or_context = |values: Vec<Value<'q>>| match values.into_iter().next() {
            None => Ok(Some(context.node)),
            Some(Value::NodeSet(nodes)) => Ok(nodes.first().copied()),
            Some(_) => Err(Error::InvalidType),
        };
        let string_or_context = |values: &mut Vec<Value<'q>>| match values.pop() {
            None => Cow::Owned(self.string_value(context.node)),
            Some(value) => self.string(value),
        };
        let strings = |values: Vec<Value<'q>>| -> Vec<Cow<'q, [u8]>> {
            values.into_iter().map(|value| self.string(value)).collect()
        };
        let boolean = |value: Value| Value::Boolean(self.truthy(&value));
        let number = |values: Vec<Value>| self.number(&values[0]);
        Ok(match function {
            Last => Value::Number(context.size as f64),
            Position => Value::Number(context.position as f64),
            Count => match &values[0] {
                Value::NodeSet(nodes) => Value::Number(nodes.len() as f64),
                _ => return Err(Error::InvalidType),
            },
            Id => Value::NodeSet(self.id(values.pop().expect("one argument"))),
            LocalName => {
                let name = node_or_context(values)?.map(|node| self.local_name_of(node));
                Value::String(Cow::Owned(name.unwrap_or_default()))
            }
            NamespaceUri => {
                let href =
                    node_or_context(values)?.and_then(|node| match self.document.node_kind(node) {
                        NodeKind::Element(_) | NodeKind::Attribute(_) => self
                            .document
                            .namespace(node)
                            .map(|ns| ns.href.as_bytes().to_vec()),
                        _ => None,
                    });
                Value::String(Cow::Owned(href.unwrap_or_default()))
            }
            Name => {
                let name = node_or_context(values)?.map(|node| self.qualified_name(node));
                Value::String(Cow::Owned(name.unwrap_or_default()))
            }
            String => Value::String(string_or_context(&mut values)),
            StringLength => {
                let string = string_or_context(&mut values);
                Value::Number(utf8_length(&string) as f64)
            }
            NormalizeSpace => {
                let string = string_or_context(&mut values);
                let words: Vec<&[u8]> = string
                    .split(|&byte| is_blank(byte))
                    .filter(|word| !word.is_empty())
                    .collect();
                Value::String(Cow::Owned(words.join(&b' ')))
            }
            Number if values.is_empty() => {
                Value::Number(string_to_number(&self.string_value(context.node)))
            }
            Number => Value::Number(number(values)),
            Concat => Value::String(Cow::Owned(strings(values).concat())),
            StartsWith => {
                let [haystack, needle] = two(strings(values));
                Value::Boolean(haystack.starts_with(&needle))
            }
            EndsWith => {
                let [haystack, needle] = two(strings(values));
                Value::Boolean(haystack.ends_with(&needle))
            }
            Contains => {
                let [haystack, needle] = two(strings(values));
                Value::Boolean(find(&haystack, &needle).is_some())
            }
            SubstringBefore => {
                let [string, needle] = two(strings(values));
                let before = find(&string, &needle).map_or(&[][..], |at| &string[..at]);
                Value::String(Cow::Owned(before.to_vec()))
            }
            SubstringAfter => {
                let [string, needle] = two(strings(values));
                let after =
                    find(&string, &needle).map_or(&[][..], |at| &string[at + needle.len()..]);
                Value::String(Cow::Owned(after.to_vec()))
            }
            Substring => {
                let length = (count == 3).then(|| self.number(&values[2]));
                let start = self.number(&values[1]);
                let string = self.string(values.swap_remove(0));
                Value::String(Cow::Owned(substring(&string, start, length)))
            }
            Translate => {
                let mut strings = strings(values).into_iter();
                let (string, from, to) = (
                    strings.next().expect("three"),
                    strings.next().expect("three"),
                    strings.next().expect("three"),
                );
                Value::String(Cow::Owned(translate(&string, &from, &to)))
            }
            Boolean => boolean(values.pop().expect("one argument")),
            Not => Value::Boolean(!self.truthy(&values[0])),
            True => Value::Boolean(true),
            False => Value::Boolean(false),
            Lang => {
                let lang = self.string(values.pop().expect("one argument"));
                Value::Boolean(self.lang(context.node, &lang))
            }
            Sum => match &values[0] {
                Value::NodeSet(nodes) => Value::Number(
                    nodes
                        .iter()
                        .map(|&node| string_to_number(&self.string_value(node)))
                        // `sum` of no floats is -0
                        .fold(0.0, |sum, n| sum + n),
                ),
                _ => return Err(Error::InvalidType),
            },
            Floor => Value::Number(number(values).floor()),
            Ceiling => Value::Number(number(values).ceil()),
            Round => Value::Number(round(number(values))),
            CssClass => {
                let [haystack, needle] = two(strings(values));
                Value::Boolean(css_class(&haystack, &needle))
            }
            LocalNameIs => {
                let name = self.string(values.pop().expect("one argument"));
                Value::Boolean(libxml2_name(self.document, context.node) == Some(&*name))
            }
            EscapeUri => {
                let reserved = self.truthy(&values[1]);
                let string = self.string(values.swap_remove(0));
                Value::String(Cow::Owned(escape_uri(&string, reserved)))
            }
        })
    }

    /// libxml2's `xmlXPathIdFunction`: the elements whose IDs are among the
    /// blank-separated tokens of the argument, or of each node's string.
    fn id(&self, argument: Value<'_>) -> Vec<NodeId> {
        let strings = match argument {
            Value::NodeSet(nodes) => nodes.iter().map(|&node| self.string_value(node)).collect(),
            value => vec![self.string(value).into_owned()],
        };
        let mut ids = self.ids.borrow_mut();
        let ids = ids.get_or_insert_with(|| self.collect_ids());
        let mut found: Vec<NodeId> = strings
            .iter()
            .flat_map(|string| string.split(|&b| is_blank(b)).filter(|t| !t.is_empty()))
            .filter_map(|token| ids.get(token).copied())
            .collect();
        self.document.sort_in_document_order(&mut found);
        found
    }

    /// libxml2 keeps the first element registered under each ID: as
    /// lobzique approximates it, the first in document order, in the
    /// document and then in the context node's tree. `xmlIsID` makes `id`
    /// attributes IDs in HTML documents, and `name` on `a` elements;
    /// otherwise only `xml:id`.
    fn collect_ids(&self) -> HashMap<Vec<u8>, NodeId> {
        let document = self.document;
        let html = document.kind() == DocumentKind::Html;
        let mut ids = HashMap::new();
        let roots = std::iter::once(NodeId::DOCUMENT)
            .chain((self.root != NodeId::DOCUMENT).then_some(self.root));
        for root in roots {
            for element in std::iter::once(root).chain(document.descendants(root)) {
                let NodeKind::Element(e) = document.node_kind(element) else {
                    continue;
                };
                for &attribute in &e.attributes {
                    let NodeKind::Attribute(a) = document.node_kind(attribute) else {
                        continue;
                    };
                    let local = &*a.name.local;
                    let is_id = if html {
                        local == "id" || (local == "name" && &*e.name.local == "a")
                    } else {
                        local == "id" && &*a.name.ns == XML_NAMESPACE
                    };
                    if is_id {
                        ids.entry(a.value.clone()).or_insert(element);
                    }
                }
            }
        }
        ids
    }

    /// libxml2's `xmlXPathLangFunction`: the `xml:lang` of the nearest
    /// element that has one, compared ignoring ASCII case up to the end or
    /// a hyphen.
    fn lang(&self, node: NodeId, lang: &[u8]) -> bool {
        let document = self.document;
        let found = std::iter::once(node)
            .chain(document.ancestors(node))
            .find_map(|node| {
                document.attributes(node).iter().find_map(|&attribute| {
                    match document.node_kind(attribute) {
                        NodeKind::Attribute(a)
                            if &*a.name.local == "lang" && &*a.name.ns == XML_NAMESPACE =>
                        {
                            Some(&a.value[..])
                        }
                        _ => None,
                    }
                })
            });
        let Some(found) = found else {
            return false;
        };
        found.len() >= lang.len()
            && found[..lang.len()].eq_ignore_ascii_case(lang)
            && matches!(found.get(lang.len()), None | Some(b'-'))
    }

    /// `local-name()`: an element's or attribute's local name, a processing
    /// instruction's target, or nothing.
    fn local_name_of(&self, node: NodeId) -> Vec<u8> {
        match self.document.node_kind(node) {
            NodeKind::Element(_) | NodeKind::Attribute(_) => {
                self.local_name(node).as_bytes().to_vec()
            }
            NodeKind::ProcessingInstruction(pi) => pi.target.clone(),
            _ => Vec::new(),
        }
    }

    /// `name()`: the local name after the namespace's prefix, if it has one.
    fn qualified_name(&self, node: NodeId) -> Vec<u8> {
        match self.document.node_kind(node) {
            NodeKind::Element(_) | NodeKind::Attribute(_) => {
                let local = self.local_name(node);
                match self.document.namespace(node).and_then(|ns| ns.prefix) {
                    Some(prefix) => format!("{prefix}:{local}").into_bytes(),
                    None => local.as_bytes().to_vec(),
                }
            }
            _ => self.local_name_of(node),
        }
    }

    fn boolean(&self, expr: &Expr, context: Context) -> Eval<bool> {
        if let Expr::Path(path) = expr {
            return self.exists(path, context.node);
        }
        Ok(self.truthy(&self.expr(expr, context)?))
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

    fn path(&self, path: &Path, context: NodeId) -> Eval<Vec<NodeId>> {
        let mut nodes = vec![if path.absolute {
            NodeId::DOCUMENT
        } else {
            context
        }];
        for step in &path.steps {
            if nodes.is_empty() {
                break;
            }
            nodes = self.step(step, &nodes)?;
        }
        Ok(nodes)
    }

    /// Whether `path` finds a node from `context`. The last step stops at
    /// the first node that passes, unless its predicates read positions,
    /// which `:has(…)` and `[@attr]` don't.
    fn exists(&self, path: &Path, context: NodeId) -> Eval<bool> {
        let Some((last, init)) = path.steps.split_last() else {
            return Ok(true);
        };
        if last.predicates.iter().any(Expr::is_positional) {
            return Ok(!self.path(path, context)?.is_empty());
        }
        let mut nodes = vec![if path.absolute {
            NodeId::DOCUMENT
        } else {
            context
        }];
        for step in init {
            if nodes.is_empty() {
                return Ok(false);
            }
            nodes = self.step(step, &nodes)?;
        }
        for node in nodes {
            for candidate in self.axis(last.axis, node) {
                if !self.passes(&last.test, last.axis, candidate) {
                    continue;
                }
                let context = Context {
                    node: candidate,
                    position: 1,
                    size: 1,
                };
                let mut passes = true;
                for predicate in &last.predicates {
                    if !self.boolean(predicate, context)? {
                        passes = false;
                        break;
                    }
                }
                if passes {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// One step from each node of `input`, which is in document order.
    fn step(&self, step: &Step, input: &[NodeId]) -> Eval<Vec<NodeId>> {
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
                candidates = self.filter(candidates, predicate)?;
            }
            if step.axis.is_reverse() {
                candidates.reverse();
            }
            output.extend(candidates);
        }
        if input.len() > 1 && !subsumes {
            document.sort_in_document_order(&mut output);
        }
        Ok(output)
    }

    fn filter(&self, candidates: Vec<NodeId>, predicate: &Expr) -> Eval<Vec<NodeId>> {
        let size = candidates.len();
        let mut kept = Vec::new();
        for (i, node) in candidates.into_iter().enumerate() {
            let context = Context {
                node,
                position: i + 1,
                size,
            };
            let passes = match predicate {
                Expr::Path(path) => self.exists(path, node)?,
                predicate => match self.expr(predicate, context)? {
                    Value::Number(n) => n == context.position as f64,
                    value => self.truthy(&value),
                },
            };
            if passes {
                kept.push(node);
            }
        }
        Ok(kept)
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
                // libxml2 starts after an attribute's element, skipping
                // what is inside it
                let start = match (is_attribute, document.parent(node)) {
                    (true, Some(element)) => element,
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
        let href = |node| self.document.namespace(node).map(|ns| ns.href);
        match test {
            // libxml2's `node()` passes neither doctypes nor fragments
            NodeTest::Node => !matches!(kind, NodeKind::Doctype(_) | NodeKind::DocumentFragment),
            NodeTest::Text => matches!(kind, NodeKind::Text(_) | NodeKind::CData(_)),
            NodeTest::Comment => matches!(kind, NodeKind::Comment(_)),
            NodeTest::ProcessingInstruction(target) => match kind {
                NodeKind::ProcessingInstruction(pi) => {
                    target.as_ref().is_none_or(|t| *t == pi.target)
                }
                _ => false,
            },
            NodeTest::Any => principal,
            NodeTest::AnyIn(prefix) => principal && href(node) == Some(self.prefixed(prefix)),
            // Nokogiri's patch covers elements only: `@*:id` finds nothing
            NodeTest::LocalName(local) => {
                matches!(kind, NodeKind::Element(_)) && self.local_name(node) == local.as_str()
            }
            NodeTest::Name(name) => {
                principal
                    && self.local_name(node) == name.local
                    && href(node) == name.prefix.as_deref().map(|prefix| self.prefixed(prefix))
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

    /// The namespace of a node test's prefix, which [`check_prefixes`] has
    /// found bound.
    fn prefixed(&self, prefix: &str) -> &str {
        lookup(self.namespaces, prefix).expect("prefixes are checked before evaluation")
    }
}

/// The two values of a function that takes two.
fn two<T>(values: Vec<T>) -> [T; 2] {
    values
        .try_into()
        .unwrap_or_else(|_| unreachable!("arity is checked"))
}

/// libxml2's `xmlStrstr`: where `needle` first starts in `haystack`. An
/// empty needle starts at 0.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// libxml2's `xmlUTF8Strlen`: the characters, counted by their lead bytes.
fn utf8_length(string: &[u8]) -> usize {
    string.iter().filter(|&&b| b & 0xC0 != 0x80).count()
}

/// The characters of `string`, each as its bytes.
fn utf8_chars(string: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut rest = string;
    std::iter::from_fn(move || {
        let (&lead, tail) = rest.split_first()?;
        let length = 1 + tail.iter().take_while(|&&b| b & 0xC0 == 0x80).count();
        let length = if lead & 0x80 == 0 { 1 } else { length };
        let (char, tail) = rest.split_at(length);
        rest = tail;
        Some(char)
    })
}

/// libxml2's `xmlXPathSubstringFunction`, which rounds the start and the
/// end, clamped to C's `int`, and counts characters from 1.
fn substring(string: &[u8], start: f64, length: Option<f64>) -> Vec<u8> {
    const INT_MAX: f64 = i32::MAX as f64;
    let round_half_up = |n: f64| {
        let floor = n.floor();
        if n - floor >= 0.5 {
            floor + 1.0
        } else {
            floor
        }
    };
    let mut first: i64 = 1;
    // NaN goes to the end, as in libxml2's `!(in < INT_MAX)`
    if start.is_nan() || start >= INT_MAX {
        first = i64::from(i32::MAX);
    } else if start >= 1.0 {
        first = round_half_up(start) as i64;
    }
    let mut end = i64::from(i32::MAX);
    if let Some(length) = length {
        let sum = round_half_up(start) + round_half_up(length);
        if sum.is_nan() || sum < 1.0 {
            end = 1;
        } else if sum < INT_MAX {
            end = sum as i64;
        }
    }
    let (first, end) = (first - 1, end - 1);
    if first >= end || first >= utf8_length(string) as i64 {
        return Vec::new();
    }
    utf8_chars(string)
        .skip(first as usize)
        .take((end - first) as usize)
        .flatten()
        .copied()
        .collect()
}

/// libxml2's `xmlXPathTranslateFunction`: each character of `string` that
/// `from` has becomes the one at the same place in `to`, or goes.
fn translate(string: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut to = utf8_chars(to);
    let mut replacements: HashMap<&[u8], Option<&[u8]>> = HashMap::new();
    for char in utf8_chars(from) {
        let replacement = to.next();
        replacements.entry(char).or_insert(replacement);
    }
    let mut translated = Vec::with_capacity(string.len());
    for char in utf8_chars(string) {
        match replacements.get(char) {
            Some(replacement) => translated.extend_from_slice(replacement.unwrap_or_default()),
            None => translated.extend_from_slice(char),
        }
    }
    translated
}

/// libxml2's `xmlXPathRoundFunction`: halves round up, and what rounds to
/// zero keeps its sign.
fn round(n: f64) -> f64 {
    if (-0.5..0.5).contains(&n) {
        return n * 0.0;
    }
    let floor = n.floor();
    if n - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// libxml2's `xmlXPathEscapeUriFunction`
fn escape_uri(string: &[u8], escape_reserved: bool) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(string.len());
    for (i, &b) in string.iter().enumerate() {
        let hex_follows = || {
            string.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
                && string.get(i + 2).is_some_and(u8::is_ascii_hexdigit)
        };
        let kept = b.is_ascii_alphanumeric()
            || b"-_.!~*'()".contains(&b)
            || (b == b'%' && hex_follows())
            || (!escape_reserved && b";/?:@&=+$,".contains(&b));
        if kept {
            escaped.push(b);
        } else {
            escaped.extend_from_slice(format!("%{b:02X}").as_bytes());
        }
    }
    escaped
}

/// The `name` libxml2 gives a node, which `local-name-is` compares.
fn libxml2_name(document: &Document, node: NodeId) -> Option<&[u8]> {
    match document.node_kind(node) {
        NodeKind::Element(element) => Some(element.name.local.as_bytes()),
        NodeKind::Attribute(attribute) => Some(attribute.name.local.as_bytes()),
        NodeKind::Text(_) => Some(b"text"),
        NodeKind::Comment(_) => Some(b"comment"),
        NodeKind::ProcessingInstruction(pi) => Some(&pi.target),
        NodeKind::Doctype(doctype) => Some(&doctype.name),
        NodeKind::CData(_) | NodeKind::Document | NodeKind::DocumentFragment => None,
    }
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

/// libxml2's `xmlXPathStringEvalNumber`: blanks, an optional minus, a
/// number as [`scan_number`] reads it, blanks; anything else is NaN.
pub fn string_to_number(s: &[u8]) -> f64 {
    let start = s.iter().position(|&b| !is_blank(b)).unwrap_or(s.len());
    let mut rest = &s[start..];
    let negative = rest.first() == Some(&b'-');
    if negative {
        rest = &rest[1..];
    }
    if !rest
        .first()
        .is_some_and(|&b| b == b'.' || b.is_ascii_digit())
    {
        return f64::NAN;
    }
    let Some((n, length)) = scan_number(rest) else {
        return f64::NAN;
    };
    if !rest[length..].iter().all(|&b| is_blank(b)) {
        return f64::NAN;
    }
    if negative {
        -n
    } else {
        n
    }
}

/// libxml2's number reading, shared by `xmlXPathCompNumber` and
/// `xmlXPathStringEvalNumber`: digits accumulated as a double, at most 20
/// fraction digits past its leading zeros, and an exponent whose digits
/// may be missing, applied with `pow`. Not correctly rounded, so long
/// numbers come out as libxml2's do. The value and the bytes read, or None
/// for a `.` with no digit on either side.
pub(super) fn scan_number(s: &[u8]) -> Option<(f64, usize)> {
    const MAX_FRAC: i32 = 20;
    let digit = |at: usize| {
        s.get(at)
            .filter(|b| b.is_ascii_digit())
            .map(|b| f64::from(b - b'0'))
    };
    let mut at = 0;
    let mut n = 0.0;
    while let Some(d) = digit(at) {
        n = n * 10.0 + d;
        at += 1;
    }
    let whole = at > 0;
    if s.get(at) == Some(&b'.') {
        at += 1;
        if !whole && digit(at).is_none() {
            return None;
        }
        let mut places = 0;
        while s.get(at) == Some(&b'0') {
            places += 1;
            at += 1;
        }
        let max = places + MAX_FRAC;
        let mut fraction = 0.0;
        while let Some(d) = digit(at).filter(|_| places < max) {
            fraction = fraction * 10.0 + d;
            places += 1;
            at += 1;
        }
        n += fraction / 10f64.powf(f64::from(places));
        while digit(at).is_some() {
            at += 1;
        }
    }
    if matches!(s.get(at), Some(b'e' | b'E')) {
        at += 1;
        let negative = s.get(at) == Some(&b'-');
        if negative || s.get(at) == Some(&b'+') {
            at += 1;
        }
        let mut exponent: i32 = 0;
        while let Some(d) = digit(at) {
            if exponent < 1_000_000 {
                exponent = exponent * 10 + d as i32;
            }
            at += 1;
        }
        if negative {
            exponent = -exponent;
        }
        n *= 10f64.powf(f64::from(exponent));
    }
    Some((n, at))
}

/// libxml2's `xmlXPathFormatNumber`: integers strictly inside C's `int`
/// range as they are, others below 1e-5 or above 1e9 as `%.14e`, the rest
/// as `%f` with 15 significant digits; trailing zeros of the fraction go.
pub fn number_to_string(n: f64) -> String {
    const INT_MIN: f64 = i32::MIN as f64;
    const INT_MAX: f64 = i32::MAX as f64;
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if n == 0.0 {
        return "0".into();
    }
    if n > INT_MIN && n < INT_MAX && n == n.trunc() {
        return format!("{}", n as i32);
    }
    let absolute = n.abs();
    if !(1e-5..=1e9).contains(&absolute) {
        // Rust writes `1.5e9` where C writes `1.5e+09`
        let formatted = format!("{n:.14e}");
        let (mantissa, exponent) = formatted.split_once('e').expect("an exponent");
        let exponent: i32 = exponent.parse().expect("a decimal exponent");
        let sign = if exponent < 0 { '-' } else { '+' };
        return format!(
            "{}e{sign}{:02}",
            trim_fraction(mantissa),
            exponent.unsigned_abs()
        );
    }
    let integer_place = absolute.log10() as i32;
    let fraction_place = if integer_place > 0 {
        15 - integer_place - 1
    } else {
        15 - integer_place
    };
    let precision = usize::try_from(fraction_place).expect("at most 20");
    trim_fraction(&format!("{n:.precision$}")).into()
}

fn trim_fraction(number: &str) -> &str {
    if number.contains('.') {
        number.trim_end_matches('0').trim_end_matches('.')
    } else {
        number
    }
}
