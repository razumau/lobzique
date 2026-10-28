//! XPath expressions as trees. `and` and `or` take lists, so the long
//! conjunctions a compound selector makes cost no stack.

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
    /// the local name in any namespace or none
    LocalName(String),
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
    Modulo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Function {
    Position,
    Last,
    Count,
    Contains,
    StartsWith,
    /// Not XPath 1.0: what Nokogiri's `substring(…)=…` test for `[a$=b]`
    /// comes to.
    EndsWith,
    Concat,
    NormalizeSpace,
    Not,
    /// Nokogiri's `nokogiri-builtin:css-class`
    CssClass,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Or(Vec<Expr>),
    And(Vec<Expr>),
    Compare(Comparison, Box<Expr>, Box<Expr>),
    Arithmetic(Arithmetic, Box<Expr>, Box<Expr>),
    Number(f64),
    Literal(Vec<u8>),
    Path(Path),
    Union(Vec<Path>),
    Call(Function, Vec<Expr>),
    /// `nokogiri:name(…)`, answered by the caller's handler; `site` numbers
    /// the call within its query.
    Custom {
        site: usize,
        arguments: Vec<Expr>,
    },
}

impl Expr {
    pub fn compare(comparison: Comparison, left: Expr, right: Expr) -> Expr {
        Expr::Compare(comparison, Box::new(left), Box::new(right))
    }

    pub fn arithmetic(operator: Arithmetic, left: Expr, right: Expr) -> Expr {
        Expr::Arithmetic(operator, Box::new(left), Box::new(right))
    }

    /// Rewrites `descendant-or-self::node()/child::x[p]`, which `//x[p]`
    /// stands for, as `descendant::x[p]` wherever no predicate reads the
    /// position: the same nodes, without visiting each one twice.
    pub(super) fn merge_descendant_steps(&mut self) {
        let merge_path = |path: &mut Path| {
            let mut merged = Vec::with_capacity(path.steps.len());
            let mut steps = std::mem::take(&mut path.steps).into_iter().peekable();
            while let Some(mut step) = steps.next() {
                for predicate in &mut step.predicates {
                    predicate.merge_descendant_steps();
                }
                let mergeable = step.axis == Axis::DescendantOrSelf
                    && step.test == NodeTest::Node
                    && step.predicates.is_empty();
                match steps.peek() {
                    Some(next)
                        if mergeable
                            && next.axis == Axis::Child
                            && !next.predicates.iter().any(Expr::is_positional) =>
                    {
                        let mut next = steps.next().expect("peeked");
                        for predicate in &mut next.predicates {
                            predicate.merge_descendant_steps();
                        }
                        next.axis = Axis::Descendant;
                        merged.push(next);
                    }
                    _ => merged.push(step),
                }
            }
            path.steps = merged;
        };
        match self {
            Expr::Or(list) | Expr::And(list) => {
                list.iter_mut().for_each(Expr::merge_descendant_steps)
            }
            Expr::Compare(_, left, right) | Expr::Arithmetic(_, left, right) => {
                left.merge_descendant_steps();
                right.merge_descendant_steps();
            }
            Expr::Call(_, arguments) | Expr::Custom { arguments, .. } => {
                arguments.iter_mut().for_each(Expr::merge_descendant_steps);
            }
            Expr::Path(path) => merge_path(path),
            Expr::Union(paths) => paths.iter_mut().for_each(merge_path),
            Expr::Number(_) | Expr::Literal(_) => {}
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
            Expr::Number(_) | Expr::Arithmetic(..) | Expr::Custom { .. } => true,
            Expr::Call(function, _) => {
                matches!(
                    function,
                    Function::Position | Function::Last | Function::Count
                )
            }
            _ => false,
        }
    }

    fn reads_position(&self) -> bool {
        match self {
            Expr::Or(list) | Expr::And(list) => list.iter().any(Expr::reads_position),
            Expr::Compare(_, left, right) | Expr::Arithmetic(_, left, right) => {
                left.reads_position() || right.reads_position()
            }
            Expr::Call(Function::Position | Function::Last, _) => true,
            Expr::Call(_, arguments) | Expr::Custom { arguments, .. } => {
                arguments.iter().any(Expr::reads_position)
            }
            Expr::Number(_) | Expr::Literal(_) | Expr::Path(_) | Expr::Union(_) => false,
        }
    }
}
