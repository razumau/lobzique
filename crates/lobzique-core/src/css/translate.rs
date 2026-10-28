//! Nokogiri's `CSS::XPathVisitor` (lib/nokogiri/css/xpath_visitor.rb),
//! building the XPath tree that the visitor writes as a string. Where the
//! visitor pastes tokens into XPath as written, this follows how libxml2
//! would read the result, down to strings that aren't valid XPath, which
//! become [`crate::xpath::Error::InvalidExpression`] for evaluation to raise, as libxml2
//! raises when Nokogiri evaluates them.

use super::parser::{
    Argument, AttributeOperator, AttributeSubject, Combinator, Condition, ElementName, Function,
    Negation, Nth, Pseudo, Selector, Simple,
};
use super::{Options, Prefix, SyntaxError};
use crate::xpath::{
    Arithmetic, Axis, Comparison, Expr, Function as F, NodeTest, Path, QName, Step,
};

pub enum Failure {
    /// Raised while translating, as Nokogiri raises from `xpath_for`.
    Syntax(SyntaxError),
    /// Translated into a string libxml2 can't parse.
    Invalid,
}

impl From<SyntaxError> for Failure {
    fn from(error: SyntaxError) -> Failure {
        Failure::Syntax(error)
    }
}

type Result<T> = std::result::Result<T, Failure>;

/// What a function or simple selector translates to, which decides where
/// the string can go.
enum Fragment {
    /// A relative location path. `axis_free` when it is a bare node test,
    /// which can follow an axis (`self::comment()`), unlike `child::text()`.
    Steps {
        steps: Vec<Step>,
        axis_free: bool,
    },
    Expr(Expr),
}

impl Fragment {
    fn into_expr(self) -> Expr {
        match self {
            Fragment::Steps { steps, .. } => Expr::Path(Path::relative(steps)),
            Fragment::Expr(expr) => expr,
        }
    }
}

/// A condition's translation, `a or b` kept apart: the visitor joins
/// conditions with a bare ` and `, which binds tighter.
enum Term {
    One(Expr),
    Or(Expr, Expr),
}

/// Where the next simple selector's string is pasted: after `/` it starts a
/// step of its own, after `::` it is the node test of a given axis.
#[derive(Clone, Copy)]
enum Join {
    Slash,
    Axis(Axis),
}

pub struct Translator<'a> {
    options: &'a Options,
    pub functions: Vec<String>,
}

impl<'a> Translator<'a> {
    pub fn new(options: &'a Options) -> Translator<'a> {
        Translator {
            options,
            functions: Vec::new(),
        }
    }

    /// `Node#to_xpath`: the prefix, or `.` for a leading `>`, `+` or `~`,
    /// then the selector.
    pub fn selector(&mut self, selector: &Selector, prefix: Prefix) -> Result<Path> {
        let (mut path, join) = match selector.leading {
            Some(Combinator::Child | Combinator::DirectAdjacent | Combinator::Following) => {
                (Path::relative(Vec::new()), None)
            }
            // `.////p` and the like
            Some(Combinator::Descendant) => return Err(Failure::Invalid),
            None => match prefix {
                Prefix::Global => (descendants(true), Some(Join::Slash)),
                Prefix::Subtree => (descendants(false), Some(Join::Slash)),
                Prefix::Current => (Path::relative(Vec::new()), Some(Join::Slash)),
                Prefix::Root => (
                    Path {
                        absolute: true,
                        steps: Vec::new(),
                    },
                    Some(Join::Slash),
                ),
                Prefix::Self_ => (Path::relative(Vec::new()), Some(Join::Axis(Axis::Self_))),
            },
        };
        let join = match (join, selector.leading) {
            (Some(join), _) => join,
            (None, Some(combinator)) => combine(&mut path, combinator),
            (None, None) => unreachable!("either a prefix or a leading combinator"),
        };
        self.chain(&mut path, join, selector)?;
        Ok(path)
    }

    /// The selector's simple selectors and combinators, appended to `path`.
    fn chain(&mut self, path: &mut Path, mut join: Join, selector: &Selector) -> Result<()> {
        self.simple(path, join, &selector.first)?;
        for (combinator, simple) in &selector.rest {
            join = combine(path, *combinator);
            self.simple(path, join, simple)?;
        }
        Ok(())
    }

    /// A simple selector pasted after `join`.
    fn simple(&mut self, path: &mut Path, join: Join, simple: &Simple) -> Result<()> {
        let (steps, axis_free, predicates) = match simple {
            Simple::Element { name, conditions } => {
                let step = Step::new(Axis::Child, self.element_test(name)?);
                let predicates = if conditions.is_empty() {
                    Vec::new()
                } else {
                    self.conditions(conditions)?
                };
                (vec![step], true, predicates)
            }
            Simple::Function {
                function,
                condition,
            } => {
                let Fragment::Steps { steps, axis_free } = self.function(function)? else {
                    // `position()=1` and the like are no steps
                    return Err(Failure::Invalid);
                };
                let predicates = match condition {
                    Some(condition) => vec![self.terms(std::slice::from_ref(condition))?],
                    None => Vec::new(),
                };
                (steps, axis_free, predicates)
            }
            Simple::Attribute(name) => (
                vec![Step::new(Axis::Attribute, NodeTest::Name(qname(name)?))],
                false,
                Vec::new(),
            ),
        };
        let mut steps = steps;
        if let Join::Axis(axis) = join {
            if !axis_free {
                return Err(Failure::Invalid);
            }
            steps[0].axis = axis;
        }
        steps
            .last_mut()
            .expect("a fragment has steps")
            .predicates
            .extend(predicates);
        path.steps.extend(steps);
        Ok(())
    }

    /// `visit_element_name`
    fn element_test(&self, name: &ElementName) -> Result<NodeTest> {
        let test = match (&name.prefix, name.local.as_str()) {
            (None, "*") => NodeTest::Any,
            // HTML5 has namespaces that CSS queries ignore
            (None, local) if self.options.html => NodeTest::LocalName(ncname(local)?.into()),
            (Some(prefix), local) => NodeTest::Name(QName {
                prefix: prefix.as_deref().map(ncname).transpose()?.map(Into::into),
                local: ncname(local)?.into(),
            }),
            (None, local) if self.options.default_namespace => NodeTest::Name(QName {
                prefix: Some("xmlns".into()),
                local: ncname(local)?.into(),
            }),
            (None, local) => NodeTest::Name(QName {
                prefix: None,
                local: ncname(local)?.into(),
            }),
        };
        Ok(test)
    }

    /// The predicates a compound's conditions make: `visit_combinator`
    /// joins them with ` and `, except that a final `*-of-type` gets
    /// predicates of its own (`][`), so it counts only the nodes that pass
    /// the conditions before it.
    fn conditions(&mut self, conditions: &[Condition]) -> Result<Vec<Expr>> {
        match conditions.split_last() {
            Some((last, init)) if !init.is_empty() && is_of_type(last) => Ok(vec![
                self.terms(init)?,
                self.terms(std::slice::from_ref(last))?,
            ]),
            _ => Ok(vec![self.terms(conditions)?]),
        }
    }

    /// Conditions joined with ` and `, read as XPath reads them.
    fn terms(&mut self, conditions: &[Condition]) -> Result<Expr> {
        let mut disjuncts: Vec<Vec<Expr>> = vec![Vec::new()];
        for condition in conditions {
            match self.condition(condition)? {
                Term::One(expr) => disjuncts.last_mut().expect("never empty").push(expr),
                Term::Or(left, right) => {
                    disjuncts.last_mut().expect("never empty").push(left);
                    disjuncts.push(vec![right]);
                }
            }
        }
        let mut conjunctions = disjuncts.into_iter().map(|mut list| {
            if list.len() == 1 {
                list.pop().expect("one")
            } else {
                Expr::And(list)
            }
        });
        Ok(if conjunctions.len() == 1 {
            conjunctions.next().expect("one")
        } else {
            Expr::Or(conjunctions.collect())
        })
    }

    fn condition(&mut self, condition: &Condition) -> Result<Term> {
        Ok(Term::One(match condition {
            // `visit_id`: /^#(.*)$/ stops at a newline
            Condition::Id(id) => {
                let id = id[1..].split('\n').next().unwrap_or_default();
                Expr::compare(Comparison::Equal, attribute("id"), literal(id, '\'')?)
            }
            Condition::Class(name) => self.css_class(attribute("class"), name)?,
            Condition::Attribute { subject, test } => {
                return self.attribute_condition(subject, test)
            }
            Condition::Pseudo(Pseudo::Ident(name)) => self.pseudo_class(name)?,
            Condition::Pseudo(Pseudo::Function(function)) => self.function(function)?.into_expr(),
            Condition::Not(Negation::Element(name)) => {
                let test = self.element_test(name)?;
                Expr::Call(
                    F::Not,
                    vec![Expr::Path(Path::relative(vec![Step::new(
                        Axis::Self_,
                        test,
                    )]))],
                )
            }
            Condition::Not(Negation::Conditions(conditions)) => {
                let predicates = self.conditions(conditions)?;
                if predicates.len() > 1 {
                    // `not(a][b)`
                    return Err(Failure::Invalid);
                }
                Expr::Call(F::Not, predicates)
            }
        }))
    }

    /// `visit_attribute_condition`
    fn attribute_condition(
        &mut self,
        subject: &AttributeSubject,
        test: &Option<(AttributeOperator, String)>,
    ) -> Result<Term> {
        let subject = match subject {
            AttributeSubject::Name(name) => Expr::Path(Path::relative(vec![Step::new(
                Axis::Attribute,
                NodeTest::Name(qname(name)?),
            )])),
            AttributeSubject::Function(function) => self.function(function)?.into_expr(),
        };
        let Some((operator, value)) = test else {
            return Ok(Term::One(subject));
        };
        let value = AttributeValue::new(value)?;
        let literal = || Expr::Literal(value.text.as_bytes().to_vec());
        let call = |function, argument: Expr| Expr::Call(function, vec![subject.clone(), argument]);
        Ok(Term::One(match operator {
            AttributeOperator::Equal => Expr::compare(Comparison::Equal, subject, literal()),
            AttributeOperator::NotEqual => Expr::compare(Comparison::NotEqual, subject, literal()),
            AttributeOperator::SubstringMatch => call(F::Contains, literal()),
            AttributeOperator::PrefixMatch => call(F::StartsWith, literal()),
            AttributeOperator::SuffixMatch => call(F::EndsWith, literal()),
            AttributeOperator::DashMatch => {
                let dashed = Expr::Call(F::Concat, vec![literal(), Expr::Literal(b"-".to_vec())]);
                return Ok(Term::Or(
                    Expr::compare(Comparison::Equal, subject.clone(), literal()),
                    call(F::StartsWith, dashed),
                ));
            }
            // The visitor strips the quotes off the literal it built and
            // quotes the rest with `'`
            AttributeOperator::Includes if value.concatenated => return Err(Failure::Invalid),
            AttributeOperator::Includes => self.css_class(subject, &value.text)?,
        }))
    }

    /// `visit_pseudo_class` for a name without parentheses.
    fn pseudo_class(&mut self, name: &str) -> Result<Expr> {
        Ok(match name {
            "first" | "first-of-type" => position_is(Expr::Number(1.0)),
            "last" | "last-of-type" => position_is(call(F::Last)),
            "first-child" => count_is(Axis::PrecedingSibling, 0.0),
            "last-child" => count_is(Axis::FollowingSibling, 0.0),
            "only-child" => Expr::And(vec![
                count_is(Axis::PrecedingSibling, 0.0),
                count_is(Axis::FollowingSibling, 0.0),
            ]),
            "only-of-type" => Expr::compare(Comparison::Equal, call(F::Last), Expr::Number(1.0)),
            "empty" => Expr::Call(F::Not, vec![children()]),
            "parent" => children(),
            "root" => Expr::Call(
                F::Not,
                vec![Expr::Path(Path::relative(vec![Step::new(
                    Axis::Parent,
                    NodeTest::Any,
                )]))],
            ),
            _ => self.custom(name, &[])?,
        })
    }

    /// `visit_function`, by the name with its `(`.
    fn function(&mut self, function: &Function) -> Result<Fragment> {
        let argument = function.arguments.first();
        let nth = match argument {
            Some(Argument::Nth(nth)) => Some(nth),
            _ => None,
        };
        let expr = match function.name.as_str() {
            "text(" => {
                return Ok(Fragment::Steps {
                    steps: vec![Step::new(Axis::Child, NodeTest::Text)],
                    axis_free: false,
                })
            }
            "self(" => {
                let Some(Argument::Ident(name)) = argument else {
                    return Err(Failure::Invalid);
                };
                return Ok(Fragment::Steps {
                    steps: vec![Step::new(Axis::Self_, NodeTest::Name(qname(name)?))],
                    axis_free: false,
                });
            }
            "comment(" => {
                return Ok(Fragment::Steps {
                    steps: vec![Step::new(Axis::Child, NodeTest::Comment)],
                    axis_free: true,
                })
            }
            "has(" => return self.has(argument),
            "eq(" => position_is(token(argument)?),
            "nth(" | "nth-of-type(" => match nth {
                Some(nth) => self::nth(nth, false, false)?,
                None => position_is(token(argument)?),
            },
            "nth-child(" => match nth {
                Some(nth) => self::nth(nth, true, false)?,
                None => count_is(Axis::PrecedingSibling, (to_i(argument)? - 1) as f64),
            },
            "nth-last-of-type(" => match nth {
                Some(nth) => self::nth(nth, false, true)?,
                None => match to_i(argument)? - 1 {
                    0 => position_is(call(F::Last)),
                    index => position_is(Expr::arithmetic(
                        Arithmetic::Subtract,
                        call(F::Last),
                        Expr::Number(index as f64),
                    )),
                },
            },
            "nth-last-child(" => match nth {
                Some(nth) => self::nth(nth, true, true)?,
                None => count_is(Axis::FollowingSibling, (to_i(argument)? - 1) as f64),
            },
            "first(" | "first-of-type(" => position_is(Expr::Number(1.0)),
            "last(" | "last-of-type(" => position_is(call(F::Last)),
            "contains(" => Expr::Call(
                F::Contains,
                vec![Expr::Path(Path::context_node()), token(argument)?],
            ),
            "gt(" => Expr::compare(Comparison::Greater, call(F::Position), token(argument)?),
            "only-child(" => Expr::compare(Comparison::Equal, call(F::Last), Expr::Number(1.0)),
            name => {
                let name = name.strip_suffix('(').unwrap_or(name);
                self.custom(name, &function.arguments)?
            }
        };
        Ok(Fragment::Expr(expr))
    }

    /// `has(…)`: `.//` and the first selector, or `.` and the selector when
    /// it has nothing before its combinator. The rest are ignored.
    fn has(&mut self, argument: Option<&Argument>) -> Result<Fragment> {
        let Some(Argument::Selector(selector)) = argument else {
            return Err(Failure::Invalid);
        };
        // The visitor checks `value[0].nil?`, which `|name` also passes
        let bare_no_namespace = selector.rest.is_empty()
            && matches!(
                &selector.first,
                Simple::Element { name: ElementName { prefix: Some(None), .. }, conditions }
                    if conditions.is_empty()
            );
        let mut path = Path::relative(vec![Step::new(Axis::Self_, NodeTest::Node)]);
        let join = match selector.leading {
            // `.foo`
            None if bare_no_namespace => return Err(Failure::Invalid),
            None => {
                path.steps
                    .push(Step::new(Axis::DescendantOrSelf, NodeTest::Node));
                Join::Slash
            }
            Some(Combinator::Descendant) => {
                path.steps
                    .push(Step::new(Axis::DescendantOrSelf, NodeTest::Node));
                Join::Slash
            }
            Some(combinator) => combine(&mut path, combinator),
        };
        self.chain(&mut path, join, selector)?;
        Ok(Fragment::Steps {
            steps: path.steps,
            axis_free: false,
        })
    }

    /// `nokogiri:name(., arguments…)`
    fn custom(&mut self, name: &str, arguments: &[Argument]) -> Result<Expr> {
        // validate_xpath_function_name
        if name.starts_with('-') {
            return Err(SyntaxError(format!("Invalid XPath function name '{name}'")).into());
        }
        let mut expressions = vec![Expr::Path(Path::context_node())];
        for argument in arguments {
            expressions.push(match argument {
                // The visitor has no `visit_nth`
                Argument::Nth(_) => {
                    return Err(SyntaxError("undefined method 'visit_nth'".into()).into())
                }
                Argument::Attribute(name) => attribute_path(name)?,
                argument => token(Some(argument))?,
            });
        }
        ncname(name)?;
        let site = self.functions.len();
        self.functions.push(name.into());
        Ok(Expr::Custom {
            site,
            arguments: expressions,
        })
    }

    /// `css_class`: the builtin, or the plain XPath 1.0 the visitor writes
    /// without builtins.
    fn css_class(&self, haystack: Expr, needle: &str) -> Result<Expr> {
        let needle = literal(needle, '\'')?;
        if self.options.builtins {
            return Ok(Expr::Call(F::CssClass, vec![haystack, needle]));
        }
        let Expr::Literal(needle) = needle else {
            unreachable!("a literal")
        };
        let space = || Expr::Literal(b" ".to_vec());
        let padded = [&b" "[..], &needle, b" "].concat();
        Ok(Expr::Call(
            F::Contains,
            vec![
                Expr::Call(
                    F::Concat,
                    vec![
                        space(),
                        Expr::Call(F::NormalizeSpace, vec![haystack]),
                        space(),
                    ],
                ),
                Expr::Literal(padded),
            ],
        ))
    }
}

/// The step a combinator adds, and where the next selector goes.
fn combine(path: &mut Path, combinator: Combinator) -> Join {
    match combinator {
        Combinator::Child => Join::Slash,
        Combinator::Descendant => {
            path.steps
                .push(Step::new(Axis::DescendantOrSelf, NodeTest::Node));
            Join::Slash
        }
        // `/following-sibling::*[1]/self::`
        Combinator::DirectAdjacent => {
            let mut sibling = Step::new(Axis::FollowingSibling, NodeTest::Any);
            sibling.predicates.push(Expr::Number(1.0));
            path.steps.push(sibling);
            Join::Axis(Axis::Self_)
        }
        Combinator::Following => Join::Axis(Axis::FollowingSibling),
    }
}

/// `//` or `.//` before the first step.
fn descendants(absolute: bool) -> Path {
    Path {
        absolute,
        steps: vec![Step::new(Axis::DescendantOrSelf, NodeTest::Node)],
    }
}

fn is_of_type(condition: &Condition) -> bool {
    let name = match condition {
        Condition::Pseudo(Pseudo::Ident(name)) => name,
        Condition::Pseudo(Pseudo::Function(function)) => &function.name,
        _ => return false,
    };
    [
        "nth-of-type",
        "first-of-type",
        "last-of-type",
        "only-of-type",
    ]
    .iter()
    .any(|of_type| name.contains(of_type))
}

/// `nth`: the position along the axis, or among element siblings for the
/// `*-child` forms, against `an+b`.
fn nth(nth: &Nth, child: bool, last: bool) -> Result<Expr> {
    let (a, b) = read_a_and_positive_b(nth)?;
    let position = || match (child, last) {
        (true, false) => Expr::arithmetic(
            Arithmetic::Add,
            count(Axis::PrecedingSibling),
            Expr::Number(1.0),
        ),
        (true, true) => Expr::arithmetic(
            Arithmetic::Add,
            count(Axis::FollowingSibling),
            Expr::Number(1.0),
        ),
        (false, false) => call(F::Position),
        (false, true) => Expr::arithmetic(
            Arithmetic::Add,
            Expr::arithmetic(Arithmetic::Subtract, call(F::Last), call(F::Position)),
            Expr::Number(1.0),
        ),
    };
    let modulo_is_zero = |expr, modulus: i128| {
        Expr::compare(
            Comparison::Equal,
            Expr::arithmetic(Arithmetic::Modulo, expr, Expr::Number(modulus as f64)),
            Expr::Number(0.0),
        )
    };
    if b == 0 {
        return Ok(modulo_is_zero(position(), a));
    }
    let comparison = if a < 0 {
        Comparison::LessOrEqual
    } else {
        Comparison::GreaterOrEqual
    };
    let bound = Expr::compare(comparison, position(), Expr::Number(b as f64));
    if a.abs() == 1 {
        return Ok(bound);
    }
    let offset = Expr::arithmetic(Arithmetic::Subtract, position(), Expr::Number(b as f64));
    Ok(Expr::And(vec![bound, modulo_is_zero(offset, a.abs())]))
}

fn read_a_and_positive_b(nth: &Nth) -> Result<(i128, i128)> {
    let a = ruby_to_i(&nth.a);
    match nth.operator.trim() {
        "+" => Ok((a, ruby_to_i(&nth.b))),
        "-" if a == 0 => Err(SyntaxError("divided by 0".into()).into()),
        "-" => Ok((a, a - ruby_modulo(ruby_to_i(&nth.b), a))),
        operator => Err(SyntaxError(format!(
            "expected an+b node to have either + or - as the operator, but is {operator:?}"
        ))
        .into()),
    }
}

/// Ruby's `%` on integers, whose result takes the divisor's sign.
fn ruby_modulo(x: i128, divisor: i128) -> i128 {
    let remainder = x.wrapping_rem(divisor);
    if remainder != 0 && (remainder < 0) != (divisor < 0) {
        remainder + divisor
    } else {
        remainder
    }
}

/// Ruby's `String#to_i`: leading whitespace, a sign, then digits that
/// single underscores may separate, and nothing for anything else. Past
/// i128, where Ruby has bignums, it saturates.
fn ruby_to_i(text: &str) -> i128 {
    let text = text.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    let (sign, digits) = match text.as_bytes().first() {
        Some(b'-') => (-1, &text[1..]),
        Some(b'+') => (1, &text[1..]),
        _ => (1, text),
    };
    let mut value: i128 = 0;
    let mut bytes = digits.bytes().peekable();
    let mut started = false;
    while let Some(byte) = bytes.next() {
        match byte {
            b'0'..=b'9' => {
                started = true;
                value = value
                    .saturating_mul(10)
                    .saturating_add(i128::from(byte - b'0'));
            }
            b'_' if started && bytes.peek().is_some_and(u8::is_ascii_digit) => {}
            _ => break,
        }
    }
    sign * value
}

/// `.to_i` on a function's first argument, as `:nth-child(3)` takes it.
fn to_i(argument: Option<&Argument>) -> Result<i128> {
    match argument {
        None => Ok(0),
        Some(Argument::Number(text) | Argument::String(text) | Argument::Ident(text)) => {
            Ok(ruby_to_i(text))
        }
        Some(_) => Err(SyntaxError("undefined method 'to_i'".into()).into()),
    }
}

/// A function argument pasted into XPath as written.
fn token(argument: Option<&Argument>) -> Result<Expr> {
    match argument {
        Some(Argument::Number(number)) => number
            .parse()
            .map(Expr::Number)
            .map_err(|_| Failure::Invalid),
        Some(Argument::String(string)) => {
            let quote = string.chars().next().expect("a quoted string");
            literal(&string[1..string.len() - 1], quote)
        }
        // A name is a child path; a leading `-` negates it
        Some(Argument::Ident(ident)) => match ident.strip_prefix('-') {
            Some(name) => Ok(Expr::arithmetic(
                Arithmetic::Subtract,
                Expr::Number(0.0),
                child_path(name)?,
            )),
            None => child_path(ident),
        },
        Some(Argument::Attribute(name)) => attribute_path(name),
        // An NTH node's #to_s, or nothing at all
        Some(Argument::Nth(_) | Argument::Selector(_)) | None => Err(Failure::Invalid),
    }
}

fn child_path(name: &str) -> Result<Expr> {
    Ok(Expr::Path(Path::relative(vec![Step::new(
        Axis::Child,
        NodeTest::Name(qname(name)?),
    )])))
}

fn attribute_path(name: &str) -> Result<Expr> {
    Ok(Expr::Path(Path::relative(vec![Step::new(
        Axis::Attribute,
        NodeTest::Name(qname(name)?),
    )])))
}

/// `@name`, for names known to be valid.
fn attribute(name: &str) -> Expr {
    Expr::Path(Path::relative(vec![Step::new(
        Axis::Attribute,
        NodeTest::Name(QName {
            prefix: None,
            local: name.into(),
        }),
    )]))
}

fn call(function: F) -> Expr {
    Expr::Call(function, Vec::new())
}

fn position_is(expr: Expr) -> Expr {
    Expr::compare(Comparison::Equal, call(F::Position), expr)
}

fn count(axis: Axis) -> Expr {
    Expr::Call(
        F::Count,
        vec![Expr::Path(Path::relative(vec![Step::new(
            axis,
            NodeTest::Any,
        )]))],
    )
}

fn count_is(axis: Axis, n: f64) -> Expr {
    Expr::compare(Comparison::Equal, count(axis), Expr::Number(n))
}

/// `node()`
fn children() -> Expr {
    Expr::Path(Path::relative(vec![Step::new(Axis::Child, NodeTest::Node)]))
}

/// `quote` + text + `quote` as an XPath literal, which can't hold its quote.
fn literal(text: &str, quote: char) -> Result<Expr> {
    if text.contains(quote) {
        return Err(Failure::Invalid);
    }
    Ok(Expr::Literal(text.as_bytes().to_vec()))
}

/// An attribute value as `visit_attribute_condition` quotes it.
struct AttributeValue {
    text: String,
    /// Quoted with `concat(…)`, because the text holds its own quote.
    concatenated: bool,
}

impl AttributeValue {
    fn new(value: &str) -> Result<AttributeValue> {
        // Unquoted values get `'…'`; a value whose first and last
        // characters are the same quote is taken as quoted
        let first = value.chars().next();
        let (text, quote) = match first {
            Some(quote @ ('\'' | '"')) => {
                if !value.ends_with(quote) {
                    return Err(Failure::Invalid);
                }
                let inner = if value.len() == 1 {
                    ""
                } else {
                    &value[1..value.len() - 1]
                };
                (inner, quote)
            }
            _ => (value, '\''),
        };
        Ok(AttributeValue {
            text: text.into(),
            concatenated: text.contains(quote),
        })
    }
}

/// `prefix:local` or `local`, each part an NCName.
fn qname(name: &str) -> Result<QName> {
    Ok(match name.split_once(':') {
        Some((prefix, local)) => QName {
            prefix: Some(ncname(prefix)?.into()),
            local: ncname(local)?.into(),
        },
        None => QName {
            prefix: None,
            local: ncname(name)?.into(),
        },
    })
}

/// XML's NCName, as libxml2's XPath parser reads names.
fn ncname(name: &str) -> Result<&str> {
    let mut chars = name.chars();
    let valid = chars.next().is_some_and(is_name_start) && chars.all(is_name_char);
    if valid {
        Ok(name)
    } else {
        Err(Failure::Invalid)
    }
}

/// XML 1.0 (fifth edition) NameStartChar, without `:`.
fn is_name_start(c: char) -> bool {
    matches!(c,
        'A'..='Z' | '_' | 'a'..='z' | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}'
        | '\u{F8}'..='\u{2FF}' | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}'
        | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}'
        | '\u{3001}'..='\u{D7FF}' | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}'
        | '\u{10000}'..='\u{EFFFF}')
}

fn is_name_char(c: char) -> bool {
    is_name_start(c)
        || matches!(c, '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}
