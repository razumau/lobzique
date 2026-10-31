//! XPath expressions as trees. Operators of one precedence take lists, so
//! a long chain of them costs no stack: only brackets nest, and the parser
//! limits how deep.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Ancestor,
    AncestorOrSelf,
    Attribute,
    Child,
    Descendant,
    DescendantOrSelf,
    Following,
    FollowingSibling,
    Parent,
    Preceding,
    PrecedingSibling,
    Self_,
}

impl Axis {
    /// Reverse axes number positions from the context node backwards.
    pub fn is_reverse(self) -> bool {
        matches!(
            self,
            Axis::Ancestor | Axis::AncestorOrSelf | Axis::Preceding | Axis::PrecedingSibling
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QName {
    pub prefix: Option<String>,
    pub local: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeTest {
    /// `node()`
    Node,
    /// `text()`, which CDATA sections pass too
    Text,
    /// `comment()`
    Comment,
    /// `*`: any node of the axis's principal type
    Any,
    /// `name` or `prefix:name`: no prefix means no namespace
    Name(QName),
    /// `*:name`, libxml2's wildcard namespace as Nokogiri patches it in:
    /// an element with the local name in any namespace or none
    LocalName(String),
    /// `prefix:*`: any node of the axis's principal type in the namespace
    AnyIn(String),
    /// `processing-instruction()`, or with a literal, those with that
    /// target
    ProcessingInstruction(Option<Vec<u8>>),
}

#[derive(Clone, Debug)]
pub struct Step {
    pub axis: Axis,
    pub test: NodeTest,
    pub predicates: Vec<Expr>,
}

impl Step {
    pub fn new(axis: Axis, test: NodeTest) -> Step {
        Step {
            axis,
            test,
            predicates: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Path {
    /// Starts at the document node rather than the context node.
    pub absolute: bool,
    pub steps: Vec<Step>,
}

impl Path {
    pub fn relative(steps: Vec<Step>) -> Path {
        Path {
            absolute: false,
            steps,
        }
    }

    /// `.`
    pub fn context_node() -> Path {
        Path::relative(vec![Step::new(Axis::Self_, NodeTest::Node)])
    }
}

/// Which steps after `descendant-or-self::node()` merge into it. The two
/// rules find the same nodes from any context node but a document
/// fragment, which `node()` doesn't pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Merge {
    /// A child step whose predicates don't read the position. CSS never
    /// searches from a fragment: Nokogiri searches its children instead.
    UnlessPositional,
    /// libxml2's `xmlXPathOptimizeExpression`, for XPath strings: a step
    /// without predicates on the child, descendant, self or
    /// descendant-or-self axis.
    WithoutPredicates,
}

impl Merge {
    /// The axis that replaces both steps, if `next` merges.
    fn axis(self, next: &Step) -> Option<Axis> {
        match self {
            Merge::UnlessPositional => (next.axis == Axis::Child
                && !next.predicates.iter().any(Expr::is_positional))
            .then_some(Axis::Descendant),
            Merge::WithoutPredicates if !next.predicates.is_empty() => None,
            Merge::WithoutPredicates => match next.axis {
                Axis::Child | Axis::Descendant => Some(Axis::Descendant),
                Axis::Self_ | Axis::DescendantOrSelf => Some(Axis::DescendantOrSelf),
                _ => None,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arithmetic {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operator {
    Compare(Comparison),
    Arithmetic(Arithmetic),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Function {
    Last,
    Position,
    Count,
    Id,
    LocalName,
    NamespaceUri,
    Name,
    String,
    Concat,
    StartsWith,
    Contains,
    SubstringBefore,
    SubstringAfter,
    Substring,
    StringLength,
    NormalizeSpace,
    Translate,
    Boolean,
    Not,
    True,
    False,
    Lang,
    Number,
    Sum,
    Floor,
    Ceiling,
    Round,
    /// Not XPath 1.0: what Nokogiri's `substring(…)=…` test for `[a$=b]`
    /// comes to.
    EndsWith,
    /// Nokogiri's `nokogiri-builtin:css-class`
    CssClass,
    /// Nokogiri's `nokogiri-builtin:local-name-is`
    LocalNameIs,
    /// libxml2's `escape-uri` in the XQuery functions namespace
    EscapeUri,
}

impl Function {
    /// The XPath 1.0 function `name` names, without a prefix.
    pub fn named(name: &str) -> Option<Function> {
        use Function::*;
        Some(match name {
            "last" => Last,
            "position" => Position,
            "count" => Count,
            "id" => Id,
            "local-name" => LocalName,
            "namespace-uri" => NamespaceUri,
            "name" => Name,
            "string" => String,
            "concat" => Concat,
            "starts-with" => StartsWith,
            "contains" => Contains,
            "substring-before" => SubstringBefore,
            "substring-after" => SubstringAfter,
            "substring" => Substring,
            "string-length" => StringLength,
            "normalize-space" => NormalizeSpace,
            "translate" => Translate,
            "boolean" => Boolean,
            "not" => Not,
            "true" => True,
            "false" => False,
            "lang" => Lang,
            "number" => Number,
            "sum" => Sum,
            "floor" => Floor,
            "ceiling" => Ceiling,
            "round" => Round,
            _ => return None,
        })
    }

    /// The function registered as `local` in the namespace `href`.
    pub fn in_namespace(href: &str, local: &str) -> Option<Function> {
        match (href, local) {
            (super::NOKOGIRI_BUILTIN_URI, "css-class") => Some(Function::CssClass),
            (super::NOKOGIRI_BUILTIN_URI, "local-name-is") => Some(Function::LocalNameIs),
            (super::XQUERY_FUNCTIONS_URI, "escape-uri") => Some(Function::EscapeUri),
            _ => None,
        }
    }

    /// Whether it may return a number, which a predicate tests against the
    /// position.
    fn may_be_number(self) -> bool {
        use Function::*;
        matches!(
            self,
            Last | Position | Count | StringLength | Number | Sum | Floor | Ceiling | Round
        )
    }
}

/// A filter expression: a primary expression, predicates that number its
/// nodes in document order, and a path from each node that passes.
#[derive(Clone, Debug)]
pub struct Filter {
    pub primary: Expr,
    pub predicates: Vec<Expr>,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Or(Vec<Expr>),
    And(Vec<Expr>),
    /// Operators of one precedence, applied left to right.
    Operation(Box<Expr>, Vec<(Operator, Expr)>),
    Negate(Box<Expr>),
    Number(f64),
    Literal(Vec<u8>),
    /// `$name`, a variable Nokogiri binds as a string.
    Variable(QName),
    Path(Path),
    Filter(Box<Filter>),
    Union(Vec<Expr>),
    Call(Function, Vec<Expr>),
    /// A function outside XPath 1.0, such as `nokogiri:name(…)`: one of
    /// the namespaced builtins, or answered by the caller's handler. `site`
    /// numbers the call within its query, which names it.
    Custom {
        site: usize,
        arguments: Vec<Expr>,
    },
}

impl Expr {
    pub fn compare(comparison: Comparison, left: Expr, right: Expr) -> Expr {
        Expr::Operation(Box::new(left), vec![(Operator::Compare(comparison), right)])
    }

    pub fn arithmetic(operator: Arithmetic, left: Expr, right: Expr) -> Expr {
        Expr::Operation(
            Box::new(left),
            vec![(Operator::Arithmetic(operator), right)],
        )
    }

    /// The expressions it evaluates in its own context: every operand but
    /// the predicates and steps of paths and filters, which start their own.
    fn operands(&self) -> Box<dyn Iterator<Item = &Expr> + '_> {
        match self {
            Expr::Or(list) | Expr::And(list) | Expr::Union(list) => Box::new(list.iter()),
            Expr::Operation(first, rest) => {
                Box::new(std::iter::once(&**first).chain(rest.iter().map(|(_, e)| e)))
            }
            Expr::Negate(expr) => Box::new(std::iter::once(&**expr)),
            Expr::Filter(filter) => Box::new(std::iter::once(&filter.primary)),
            Expr::Call(_, arguments) | Expr::Custom { arguments, .. } => Box::new(arguments.iter()),
            Expr::Number(_) | Expr::Literal(_) | Expr::Variable(_) | Expr::Path(_) => {
                Box::new(std::iter::empty())
            }
        }
    }

    fn operands_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expr> + '_> {
        match self {
            Expr::Or(list) | Expr::And(list) | Expr::Union(list) => Box::new(list.iter_mut()),
            Expr::Operation(first, rest) => {
                Box::new(std::iter::once(&mut **first).chain(rest.iter_mut().map(|(_, e)| e)))
            }
            Expr::Negate(expr) => Box::new(std::iter::once(&mut **expr)),
            Expr::Filter(filter) => Box::new(std::iter::once(&mut filter.primary)),
            Expr::Call(_, arguments) | Expr::Custom { arguments, .. } => {
                Box::new(arguments.iter_mut())
            }
            Expr::Number(_) | Expr::Literal(_) | Expr::Variable(_) | Expr::Path(_) => {
                Box::new(std::iter::empty())
            }
        }
    }

    /// Each step in it, at any depth.
    pub(super) fn for_each_step(&self, each: &mut dyn FnMut(&Step)) {
        let walk = |steps: &[Step], each: &mut dyn FnMut(&Step)| {
            for step in steps {
                each(step);
                for predicate in &step.predicates {
                    predicate.for_each_step(each);
                }
            }
        };
        match self {
            Expr::Path(path) => walk(&path.steps, each),
            Expr::Filter(filter) => {
                filter.primary.for_each_step(each);
                for predicate in &filter.predicates {
                    predicate.for_each_step(each);
                }
                walk(&filter.steps, each);
            }
            expr => expr.operands().for_each(|e| e.for_each_step(each)),
        }
    }

    /// Rewrites `descendant-or-self::node()/child::x[p]`, which `//x[p]`
    /// stands for, as `descendant::x[p]`: the same nodes, without visiting
    /// each one twice.
    pub(super) fn merge_descendant_steps(&mut self, merge: Merge) {
        let merge_path = |path: &mut Vec<Step>| {
            let mut merged = Vec::with_capacity(path.len());
            let mut steps = std::mem::take(path).into_iter().peekable();
            while let Some(mut step) = steps.next() {
                for predicate in &mut step.predicates {
                    predicate.merge_descendant_steps(merge);
                }
                let mergeable = step.axis == Axis::DescendantOrSelf
                    && step.test == NodeTest::Node
                    && step.predicates.is_empty();
                let axis = match steps.peek() {
                    Some(next) if mergeable => merge.axis(next),
                    _ => None,
                };
                match axis {
                    Some(axis) => {
                        let mut next = steps.next().expect("peeked");
                        for predicate in &mut next.predicates {
                            predicate.merge_descendant_steps(merge);
                        }
                        next.axis = axis;
                        merged.push(next);
                    }
                    None => merged.push(step),
                }
            }
            *path = merged;
        };
        match self {
            Expr::Path(path) => merge_path(&mut path.steps),
            Expr::Filter(filter) => {
                filter.primary.merge_descendant_steps(merge);
                for predicate in &mut filter.predicates {
                    predicate.merge_descendant_steps(merge);
                }
                merge_path(&mut filter.steps);
            }
            expr => expr
                .operands_mut()
                .for_each(|e| e.merge_descendant_steps(merge)),
        }
    }

    /// Whether a predicate's answer can depend on the position of the node
    /// it tests: a number tests the position itself, and `position()` and
    /// `last()` read it. Paths start their own contexts.
    pub fn is_positional(&self) -> bool {
        self.may_be_number() || self.reads_position()
    }

    fn may_be_number(&self) -> bool {
        match self {
            Expr::Number(_) | Expr::Negate(_) | Expr::Custom { .. } => true,
            Expr::Operation(_, rest) => {
                matches!(rest.last(), Some((Operator::Arithmetic(_), _)))
            }
            Expr::Call(function, _) => function.may_be_number(),
            _ => false,
        }
    }

    fn reads_position(&self) -> bool {
        match self {
            Expr::Call(Function::Position | Function::Last, _) => true,
            expr => expr.operands().any(Expr::reads_position),
        }
    }
}
