//! XPath expressions as trees. Operators of one precedence take lists,
//! so a long chain of them uses no extra stack. Only brackets nest,
//! and the parser limits their depth.

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
    Node,
    /// `text()`. CDATA sections pass it too.
    Text,
    Comment,
    /// `*`: any node of the principal type of the axis.
    Any,
    /// `name` or `prefix:name`. No prefix means no namespace.
    Name(QName),
    /// `*:name`, the wildcard namespace of libxml2 as Nokogiri patches
    /// it: an element with the local name in any namespace or in none.
    LocalName(String),
    /// `prefix:*`: any node of the principal type of the axis in the namespace.
    AnyIn(String),
    /// `processing-instruction()`. With a literal, only those with that target.
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
    /// Starts at the document node, not the context node.
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

    pub fn context_node() -> Path {
        Path::relative(vec![Step::new(Axis::Self_, NodeTest::Node)])
    }
}

/// The steps after `descendant-or-self::node()` that merge
/// into it. From any context node except a document fragment, both rules find
/// the same nodes. A fragment differs because `node()` does not pass it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Merge {
    /// A child step whose predicates do not read the position. CSS never
    /// searches from a fragment: Nokogiri searches its children instead.
    UnlessPositional,
    /// libxml2's `xmlXPathOptimizeExpression`, for XPath strings: a step
    /// without predicates on the child, descendant, self or
    /// descendant-or-self axis.
    WithoutPredicates,
}

impl Merge {
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
    /// Not XPath 1.0. The result of the `substring(…)=…` test that Nokogiri
    /// uses for `[a$=b]`.
    EndsWith,
    /// Nokogiri's `nokogiri-builtin:css-class`
    CssClass,
    /// Nokogiri's `nokogiri-builtin:local-name-is`
    LocalNameIs,
    /// `escape-uri` of libxml2, in the XQuery functions namespace.
    EscapeUri,
}

impl Function {
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

    pub fn in_namespace(href: &str, local: &str) -> Option<Function> {
        match (href, local) {
            (super::NOKOGIRI_BUILTIN_URI, "css-class") => Some(Function::CssClass),
            (super::NOKOGIRI_BUILTIN_URI, "local-name-is") => Some(Function::LocalNameIs),
            (super::XQUERY_FUNCTIONS_URI, "escape-uri") => Some(Function::EscapeUri),
            _ => None,
        }
    }

    /// Whether it can return a number. A predicate compares a number
    /// with the position.
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
    Operation(Box<Expr>, Vec<(Operator, Expr)>),
    Negate(Box<Expr>),
    Number(f64),
    Literal(Vec<u8>),
    /// `$name`: a variable that Nokogiri binds as a string.
    Variable(QName),
    Path(Path),
    Filter(Box<Filter>),
    Union(Vec<Expr>),
    Call(Function, Vec<Expr>),
    /// A function outside XPath 1.0, for example `nokogiri:name(…)`.
    /// It is a namespaced builtin, or the handler of the caller answers
    /// it. `site` numbers the call in its query, which names it.
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

    /// The expressions that it evaluates in its own context: all operands
    /// except the predicates and steps of paths and filters, which start
    /// their own.
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

    /// Rewrites `descendant-or-self::node()/child::x[p]`, which `//x[p]` stands
    /// for, as `descendant::x[p]`. It finds the same nodes without visiting
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

    /// Whether the answer of a predicate can depend on the position of the node
    /// it tests. A number tests the position itself, and `position()`
    /// and `last()` read it. Paths start their own contexts.
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
