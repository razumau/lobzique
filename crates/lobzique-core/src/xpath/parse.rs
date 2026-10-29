//! XPath strings, parsed into the trees [`evaluate`](super::evaluate) runs.
//!
//! lobzique evaluates a subset of XPath (plan.md decision 2): location paths
//! on every axis but `namespace`, whose predicates are positions, paths, or
//! `=` between paths, literals and numbers. The parser still reads all of
//! XPath 1.0 as libxml2 reads it, so an expression libxml2 rejects fails
//! with libxml2's error, and one it accepts that the subset leaves out
//! fails with [`ParseError::Unsupported`], naming what it is.

use super::ast::{Axis, Comparison, Expr, NodeTest, Path, QName, Step};
use super::chars::{is_blank, is_name_char, is_name_start, is_xml_char};
use super::{Error, Query};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// libxml2 can't compile it either.
    Syntax(Error),
    /// Valid XPath outside the subset: the construct it uses.
    Unsupported(String),
}

impl From<Error> for ParseError {
    fn from(error: Error) -> ParseError {
        ParseError::Syntax(error)
    }
}

/// Predicates and parentheses nest at most this deep, as CSS does: each
/// level costs stack to parse and evaluate.
pub const MAX_NESTING: usize = 32;

pub fn parse(expression: &[u8]) -> Result<Query> {
    let mut parser = Parser {
        input: expression,
        at: 0,
        depth: 0,
        unsupported: None,
    };
    parser.skip_blanks();
    let expr = parser.expr()?;
    parser.skip_blanks();
    if parser.at < parser.input.len() {
        return Err(Error::InvalidExpression.into());
    }
    if let Some(construct) = parser.unsupported {
        return Err(ParseError::Unsupported(construct));
    }
    if !matches!(expr, Expr::Path(_)) {
        return Err(ParseError::Unsupported(
            "expressions that aren't location paths".into(),
        ));
    }
    Ok(Query::xpath(expr))
}

type Result<T> = std::result::Result<T, ParseError>;

struct Parser<'a> {
    input: &'a [u8],
    at: usize,
    depth: usize,
    /// The first construct outside the subset, in reading order. Parsing
    /// carries on, so a syntax error later still wins, as libxml2 would
    /// raise it.
    unsupported: Option<String>,
}

/// What stands in for a construct outside the subset. It is never
/// evaluated, and keeps the tree flat.
const PLACEHOLDER: Expr = Expr::Number(0.0);

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.input.get(self.at).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.input.get(self.at + offset).copied()
    }

    fn rest(&self) -> &[u8] {
        &self.input[self.at..]
    }

    /// libxml2's `SKIP_BLANKS`
    fn skip_blanks(&mut self) {
        while self.peek().is_some_and(is_blank) {
            self.at += 1;
        }
    }

    fn eat(&mut self, token: &[u8]) -> bool {
        let found = self.rest().starts_with(token);
        if found {
            self.at += token.len();
        }
        found
    }

    fn unsupported(&mut self, construct: impl Into<String>) -> Expr {
        self.unsupported.get_or_insert_with(|| construct.into());
        PLACEHOLDER
    }

    /// Called past an opening bracket: whether to parse what it holds. Past
    /// [`MAX_NESTING`], it skips to the matching bracket instead, so the
    /// stack stays bounded and an unclosed bracket or literal still raises
    /// libxml2's error. Other syntax errors in what it skips go unreported.
    fn nest(&mut self) -> Result<bool> {
        if self.depth == MAX_NESTING {
            self.unsupported(format!("nesting deeper than {MAX_NESTING}"));
            self.skip_nested()?;
            return Ok(false);
        }
        self.depth += 1;
        Ok(true)
    }

    fn skip_nested(&mut self) -> Result<()> {
        let mut open = vec![self.input[self.at - 1]];
        while let Some(&innermost) = open.last() {
            let unclosed = || match innermost {
                b'[' => Error::InvalidPredicate,
                _ => Error::InvalidExpression,
            };
            match self.peek() {
                None => return Err(unclosed().into()),
                Some(b'\'' | b'"') => {
                    self.literal()?;
                    continue;
                }
                Some(b @ (b'[' | b'(')) => open.push(b),
                Some(b']') if innermost == b'[' => _ = open.pop(),
                Some(b')') if innermost == b'(' => _ = open.pop(),
                Some(b']' | b')') => return Err(unclosed().into()),
                Some(_) => {}
            }
            self.at += 1;
        }
        Ok(())
    }

    fn expr(&mut self) -> Result<Expr> {
        let mut expr = self.and_expr()?;
        // libxml2 matches operator names without looking past them
        while self.skip_operator(b"or") {
            expr = self.unsupported("the or operator");
            self.and_expr()?;
        }
        Ok(expr)
    }

    fn and_expr(&mut self) -> Result<Expr> {
        let mut expr = self.equality_expr()?;
        while self.skip_operator(b"and") {
            expr = self.unsupported("the and operator");
            self.equality_expr()?;
        }
        Ok(expr)
    }

    /// Skips blanks, and `operator` if it comes next.
    fn skip_operator(&mut self, operator: &[u8]) -> bool {
        self.skip_blanks();
        let found = self.eat(operator);
        self.skip_blanks();
        found
    }

    fn equality_expr(&mut self) -> Result<Expr> {
        let mut expr = self.relational_expr()?;
        loop {
            self.skip_blanks();
            let equal = if self.eat(b"=") {
                true
            } else if self.eat(b"!=") {
                false
            } else {
                return Ok(expr);
            };
            self.skip_blanks();
            let supported = if !equal {
                self.unsupported("the != operator");
                false
            } else if matches!(expr, Expr::Compare(..)) {
                self.unsupported("chained comparisons");
                false
            } else {
                true
            };
            let right = self.relational_expr()?;
            expr = if supported {
                Expr::compare(Comparison::Equal, expr, right)
            } else {
                PLACEHOLDER
            };
        }
    }

    fn relational_expr(&mut self) -> Result<Expr> {
        let mut expr = self.additive_expr()?;
        loop {
            self.skip_blanks();
            let operator = [&b"<="[..], b"<", b">=", b">"]
                .into_iter()
                .find(|operator| self.rest().starts_with(operator));
            let Some(operator) = operator else {
                return Ok(expr);
            };
            self.at += operator.len();
            let operator = String::from_utf8_lossy(operator);
            expr = self.unsupported(format!("the {operator} operator"));
            self.skip_blanks();
            self.additive_expr()?;
        }
    }

    fn additive_expr(&mut self) -> Result<Expr> {
        let mut expr = self.multiplicative_expr()?;
        loop {
            self.skip_blanks();
            let operator = match self.peek() {
                Some(b'+') => "+",
                Some(b'-') => "-",
                _ => return Ok(expr),
            };
            self.at += 1;
            expr = self.unsupported(format!("the {operator} operator"));
            self.skip_blanks();
            self.multiplicative_expr()?;
        }
    }

    fn multiplicative_expr(&mut self) -> Result<Expr> {
        let mut expr = self.unary_expr()?;
        loop {
            self.skip_blanks();
            let operator = [&b"*"[..], b"div", b"mod"]
                .into_iter()
                .find(|operator| self.rest().starts_with(operator));
            let Some(operator) = operator else {
                return Ok(expr);
            };
            self.at += operator.len();
            let operator = String::from_utf8_lossy(operator);
            expr = self.unsupported(format!("the {operator} operator"));
            self.skip_blanks();
            self.unary_expr()?;
        }
    }

    /// Minus signs before a number are part of it; before anything else,
    /// they are arithmetic.
    fn unary_expr(&mut self) -> Result<Expr> {
        self.skip_blanks();
        let mut negative = false;
        let mut signed = false;
        while self.peek() == Some(b'-') {
            self.at += 1;
            negative = !negative;
            signed = true;
            self.skip_blanks();
        }
        let expr = self.union_expr()?;
        Ok(match expr {
            Expr::Number(n) if negative => Expr::Number(-n),
            expr if !signed || matches!(expr, Expr::Number(_)) => expr,
            _ => self.unsupported("unary minus"),
        })
    }

    fn union_expr(&mut self) -> Result<Expr> {
        let mut expr = self.path_expr()?;
        loop {
            self.skip_blanks();
            if !self.eat(b"|") {
                return Ok(expr);
            }
            expr = self.unsupported("unions");
            self.skip_blanks();
            self.path_expr()?;
        }
    }

    /// libxml2's `xmlXPathCompPathExpr`: a location path, or a primary
    /// expression with optional predicates and path after it.
    fn path_expr(&mut self) -> Result<Expr> {
        self.skip_blanks();
        let starts_primary = match self.peek() {
            Some(b'$' | b'(' | b'\'' | b'"' | b'0'..=b'9') => true,
            Some(b'.') => self.peek_at(1).is_some_and(|b| b.is_ascii_digit()),
            _ => self.function_name_ahead()?,
        };
        if !starts_primary {
            return Ok(Expr::Path(self.location_path()?));
        }
        let primary = self.primary_expr()?;
        self.skip_blanks();
        let mut filtered = false;
        while self.peek() == Some(b'[') {
            self.predicate()?;
            filtered = true;
            self.skip_blanks();
        }
        if self.eat(b"//") || self.eat(b"/") {
            self.skip_blanks();
            self.relative_path(Vec::new())?;
            filtered = true;
        }
        Ok(if filtered {
            self.unsupported("filter expressions")
        } else {
            primary
        })
    }

    /// Whether a function call starts here: a QName, other than a node
    /// type, followed by `(`.
    fn function_name_ahead(&mut self) -> Result<bool> {
        let start = self.at;
        let ahead = (|| {
            if !self.scan_ncname(|_| ())? {
                return Ok(false);
            }
            if self.peek() == Some(b':') && self.peek_at(1) != Some(b':') {
                self.at += 1;
                if !self.scan_ncname(|_| ())? {
                    return Ok(false);
                }
            } else if is_node_type(&self.input[start..self.at]) {
                return Ok(false);
            }
            self.skip_blanks();
            Ok(self.peek() == Some(b'('))
        })();
        self.at = start;
        ahead
    }

    fn primary_expr(&mut self) -> Result<Expr> {
        match self.peek() {
            Some(b'$') => {
                self.at += 1;
                let name = self.qname()?.ok_or(Error::VariableReference)?;
                Ok(self.unsupported(format!("variable ${name}")))
            }
            Some(b'(') => {
                self.at += 1;
                let placeholder = self.unsupported("parentheses");
                if !self.nest()? {
                    return Ok(placeholder);
                }
                self.skip_blanks();
                self.expr()?;
                self.skip_blanks();
                if !self.eat(b")") {
                    return Err(Error::InvalidExpression.into());
                }
                self.depth -= 1;
                Ok(placeholder)
            }
            Some(b'\'' | b'"') => Ok(Expr::Literal(self.literal()?)),
            Some(b'0'..=b'9' | b'.') => Ok(Expr::Number(self.number())),
            _ => self.function_call(),
        }
    }

    fn function_call(&mut self) -> Result<Expr> {
        let name = self.qname()?.ok_or(Error::InvalidExpression)?;
        let placeholder = self.unsupported(format!("function {name}()"));
        self.skip_blanks();
        self.at += 1; // `(`, as function_name_ahead saw
        if !self.nest()? {
            return Ok(placeholder);
        }
        self.skip_blanks();
        if self.peek() != Some(b')') {
            loop {
                self.expr()?;
                self.skip_blanks();
                if self.peek() == Some(b')') {
                    break;
                }
                if !self.eat(b",") {
                    return Err(Error::InvalidExpression.into());
                }
                self.skip_blanks();
            }
        }
        self.at += 1;
        self.depth -= 1;
        Ok(placeholder)
    }

    /// libxml2's `xmlXPathCompLocationPath`, which reads `/` again after a
    /// `/` that no step follows: `/ /p` is `/p`.
    fn location_path(&mut self) -> Result<Path> {
        if self.peek() != Some(b'/') {
            return Ok(Path::relative(self.relative_path(Vec::new())?));
        }
        let mut steps = Vec::new();
        while self.peek() == Some(b'/') {
            if self.eat(b"//") {
                self.skip_blanks();
                steps.push(descendant_or_self());
                steps = self.relative_path(steps)?;
            } else {
                self.at += 1;
                self.skip_blanks();
                if self.step_ahead() {
                    steps = self.relative_path(steps)?;
                }
            }
        }
        Ok(Path {
            absolute: true,
            steps,
        })
    }

    /// Whether libxml2 reads a step after `/`, from the byte alone.
    fn step_ahead(&self) -> bool {
        self.peek().is_some_and(|b| {
            b.is_ascii_alphabetic() || b >= 0x80 || matches!(b, b'_' | b'.' | b'@' | b'*')
        })
    }

    fn relative_path(&mut self, mut steps: Vec<Step>) -> Result<Vec<Step>> {
        steps.extend(self.step()?);
        loop {
            self.skip_blanks();
            if self.eat(b"//") {
                steps.push(descendant_or_self());
            } else if !self.eat(b"/") {
                return Ok(steps);
            }
            self.skip_blanks();
            steps.extend(self.step()?);
        }
    }

    /// libxml2's `xmlXPathCompStep`. `.` is no step at all, rather than
    /// `self::node()`, which a fragment doesn't pass.
    fn step(&mut self) -> Result<Option<Step>> {
        if self.eat(b"..") {
            return Ok(Some(Step::new(Axis::Parent, NodeTest::Node)));
        }
        if self.eat(b".") {
            return Ok(None);
        }
        let (axis, name) = if self.eat(b"@") {
            self.skip_blanks();
            (Axis::Attribute, None)
        } else if let Some(name) = self.ncname()? {
            self.skip_blanks();
            if self.eat(b"::") {
                self.skip_blanks();
                (self.axis(&name)?, None)
            } else {
                (Axis::Child, Some(name))
            }
        } else {
            (Axis::Child, None)
        };
        let test = self.node_test(name)?;
        let mut step = Step::new(axis, test);
        loop {
            self.skip_blanks();
            if self.peek() != Some(b'[') {
                return Ok(Some(step));
            }
            let predicate = self.predicate()?;
            step.predicates.push(predicate);
        }
    }

    /// The axis `name::` names.
    fn axis(&mut self, name: &str) -> Result<Axis> {
        Ok(match name {
            "ancestor" => Axis::Ancestor,
            "ancestor-or-self" => Axis::AncestorOrSelf,
            "attribute" => Axis::Attribute,
            "child" => Axis::Child,
            "descendant" => Axis::Descendant,
            "descendant-or-self" => Axis::DescendantOrSelf,
            "following" => Axis::Following,
            "following-sibling" => Axis::FollowingSibling,
            "parent" => Axis::Parent,
            "preceding" => Axis::Preceding,
            "preceding-sibling" => Axis::PrecedingSibling,
            "self" => Axis::Self_,
            "namespace" => {
                self.unsupported("the namespace axis");
                Axis::Child
            }
            _ => return Err(Error::InvalidExpression.into()),
        })
    }

    /// libxml2's `xmlXPathCompNodeTest`, after `name` and the blanks after
    /// it if the step already read them. Blanks may come before the colon
    /// of a QName, which libxml2 skips while looking for `::`, but not
    /// after it.
    fn node_test(&mut self, name: Option<String>) -> Result<NodeTest> {
        let name = match name {
            Some(name) => name,
            None => {
                if self.eat(b"*") {
                    // Nokogiri's patch: `*:name` is the name in any namespace
                    if !self.eat(b":") {
                        return Ok(NodeTest::Any);
                    }
                    if self.eat(b"*") {
                        return Ok(NodeTest::LocalName("*".into()));
                    }
                    let local = self.ncname()?.ok_or(Error::InvalidExpression)?;
                    return Ok(NodeTest::LocalName(local));
                }
                let name = self.ncname()?.ok_or(Error::InvalidExpression)?;
                self.skip_blanks();
                name
            }
        };
        if self.eat(b"(") {
            return self.node_type(&name);
        }
        if !self.eat(b":") {
            return Ok(NodeTest::Name(QName {
                prefix: None,
                local: name,
            }));
        }
        if self.eat(b"*") {
            self.unsupported(format!("the node test {name}:*"));
            return Ok(NodeTest::Any);
        }
        let local = self.ncname()?.ok_or(Error::InvalidExpression)?;
        Ok(NodeTest::Name(QName {
            prefix: Some(name),
            local,
        }))
    }

    /// The node test `name(`, after the parenthesis.
    fn node_type(&mut self, name: &str) -> Result<NodeTest> {
        self.skip_blanks();
        let test = match name {
            "node" => NodeTest::Node,
            "text" => NodeTest::Text,
            "comment" => NodeTest::Comment,
            "processing-instruction" => {
                if matches!(self.peek(), Some(b'\'' | b'"')) {
                    self.literal()?;
                    self.skip_blanks();
                } else if self.peek() != Some(b')') {
                    return Err(Error::StartLiteral.into());
                }
                self.unsupported("processing-instruction()");
                NodeTest::Node
            }
            _ => return Err(Error::InvalidExpression.into()),
        };
        if !self.eat(b")") {
            return Err(Error::Unclosed.into());
        }
        Ok(test)
    }

    fn predicate(&mut self) -> Result<Expr> {
        self.at += 1; // `[`
        if !self.nest()? {
            return Ok(PLACEHOLDER);
        }
        self.skip_blanks();
        let expr = self.expr()?;
        self.skip_blanks();
        if !self.eat(b"]") {
            return Err(Error::InvalidPredicate.into());
        }
        self.depth -= 1;
        Ok(match expr {
            Expr::Literal(_) => self.unsupported("literals as predicates"),
            expr => expr,
        })
    }

    /// libxml2's `xmlXPathParseLiteral`: no escapes, any XML character,
    /// checked as it reads, so a bad one before the end is reported
    /// rather than the missing quote.
    fn literal(&mut self) -> Result<Vec<u8>> {
        let quote = self.input[self.at];
        self.at += 1;
        let start = self.at;
        loop {
            match self.peek() {
                None => return Err(Error::UnfinishedLiteral.into()),
                Some(b) if b == quote => break,
                // `xmlGetUTF8Char`, which unlike `xmlXPathCurrentChar`
                // rejects these leads and overlong forms
                Some(0x80..=0xC1) => return Err(Error::InvalidChar.into()),
                Some(_) => {}
            }
            let (value, length) = decode(self.rest()).ok_or(Error::InvalidChar)?;
            let shortest = [0, 0, 0x80, 0x800, 0x10000][length];
            let valid = value >= shortest && char::from_u32(value).is_some_and(is_xml_char);
            if !valid {
                return Err(Error::InvalidChar.into());
            }
            self.at += length;
        }
        let content = self.input[start..self.at].to_vec();
        self.at += 1;
        Ok(content)
    }

    /// libxml2's `xmlXPathCompNumber`: digits, an optional fraction, and an
    /// optional exponent, whose digits may be missing.
    fn number(&mut self) -> f64 {
        let digits = |parser: &mut Self| {
            while parser.peek().is_some_and(|b| b.is_ascii_digit()) {
                parser.at += 1;
            }
        };
        let start = self.at;
        digits(self);
        if self.eat(b".") {
            digits(self);
        }
        let mantissa = &self.input[start..self.at];
        let mut exponent = 0i32;
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            let negative = self.eat(b"-");
            if !negative {
                self.eat(b"+");
            }
            let start = self.at;
            digits(self);
            exponent = std::str::from_utf8(&self.input[start..self.at])
                .ok()
                .and_then(|e| e.parse().ok())
                .unwrap_or(if start == self.at { 0 } else { i32::MAX });
            if negative {
                exponent = -exponent;
            }
        }
        let mantissa = std::str::from_utf8(mantissa).expect("ASCII digits");
        format!("{mantissa}e{exponent}").parse().unwrap_or(f64::NAN)
    }

    /// libxml2's `xmlXPathParseQName`: None if either part is missing.
    fn qname(&mut self) -> Result<Option<String>> {
        let Some(name) = self.ncname()? else {
            return Ok(None);
        };
        if !self.eat(b":") {
            return Ok(Some(name));
        }
        Ok(self.ncname()?.map(|local| format!("{name}:{local}")))
    }

    fn ncname(&mut self) -> Result<Option<String>> {
        let mut name = String::new();
        Ok(self.scan_ncname(|c| name.push(c))?.then_some(name))
    }

    /// Reads an NCName, if one starts here, passing on each character as
    /// decoded: an overlong form is the character it encodes.
    fn scan_ncname(&mut self, mut each: impl FnMut(char)) -> Result<bool> {
        let Some((first, length)) = self.char_here()? else {
            return Ok(false);
        };
        if !is_name_start(first) {
            return Ok(false);
        }
        each(first);
        self.at += length;
        while let Some((c, length)) = self.char_here()? {
            if !is_name_char(c) {
                break;
            }
            each(c);
            self.at += length;
        }
        Ok(true)
    }

    /// libxml2's `xmlXPathCurrentChar`: the character here and its length
    /// in bytes. Bytes that don't form a sequence are an encoding error; a
    /// sequence that isn't an XML `Char`, an overlong `NUL` or a surrogate
    /// say, is an invalid character.
    fn char_here(&self) -> Result<Option<(char, usize)>> {
        let rest = self.rest();
        match rest.first() {
            None => return Ok(None),
            Some(&b) if b.is_ascii() => return Ok(Some((char::from(b), 1))),
            Some(0xF8..) => return Err(Error::Encoding.into()),
            Some(_) => {}
        }
        let (value, length) = decode(rest).ok_or(Error::Encoding)?;
        let c = char::from_u32(value)
            .filter(|&c| is_xml_char(c))
            .ok_or(Error::InvalidChar)?;
        Ok(Some((c, length)))
    }
}

/// The code point and length of the UTF-8 sequence that starts `bytes`, as
/// libxml2 reads one: the lead byte sets the length, and the value isn't
/// checked. None if a continuation byte is missing.
fn decode(bytes: &[u8]) -> Option<(u32, usize)> {
    let lead = bytes[0];
    let length = match lead {
        0..=0x7F => return Some((lead.into(), 1)),
        0x80..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    };
    let continuations = bytes.get(1..length)?;
    if !continuations.iter().all(|b| b & 0xC0 == 0x80) {
        return None;
    }
    let value = continuations
        .iter()
        .fold(u32::from(lead) & (0x7F >> length), |value, b| {
            value << 6 | u32::from(b & 0x3F)
        });
    Some((value, length))
}

fn descendant_or_self() -> Step {
    Step::new(Axis::DescendantOrSelf, NodeTest::Node)
}

fn is_node_type(name: &[u8]) -> bool {
    matches!(
        name,
        b"node" | b"text" | b"comment" | b"processing-instruction"
    )
}
