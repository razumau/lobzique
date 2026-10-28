//! Nokogiri's CSS parser: racc's parse loop over the tables generated from
//! Nokogiri's grammar (`tables.rs`), with the grammar's actions
//! (lib/nokogiri/css/parser.y) building a typed tree instead of
//! `Nokogiri::CSS::Node`s. Chains that the grammar builds right-recursively,
//! `a b c` and `.a.b.c`, are kept flat, so long selectors cost no stack.

use super::tables::{self as t, N};
use super::tokenizer::{Token, Tokenizer};
use super::SyntaxError;

/// Parentheses nest at most this deep, which bounds the stack the parsed
/// tree takes to translate, evaluate and drop: about 4 KB a level in a
/// release build. Nokogiri sets no limit.
pub const MAX_NESTING: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combinator {
    Child,
    Descendant,
    DirectAdjacent,
    Following,
}

/// One selector of a comma-separated list, the node Nokogiri calls
/// `to_xpath` on.
#[derive(Debug)]
pub struct Selector {
    /// `> p`: a combinator with nothing before it.
    pub leading: Option<Combinator>,
    pub first: Simple,
    pub rest: Vec<(Combinator, Simple)>,
}

#[derive(Debug)]
pub enum Simple {
    /// `p.a`, or `.a` with `*` implied. Nokogiri's `CONDITIONAL_SELECTOR`
    /// when there are conditions.
    Element {
        name: ElementName,
        conditions: Vec<Condition>,
    },
    /// `text()`, `comment()`, `has(…)`, with at most one condition.
    Function {
        function: Function,
        condition: Option<Condition>,
    },
    /// `@href`
    Attribute(String),
}

#[derive(Debug)]
pub struct ElementName {
    /// `None` for `name`, `Some(None)` for `|name`, `Some(Some(ns))` for
    /// `ns|name`.
    pub prefix: Option<Option<String>>,
    /// `*` for the universal selector.
    pub local: String,
}

#[derive(Debug)]
pub enum Condition {
    /// The `#id` token, unescaped, `#` included.
    Id(String),
    Class(String),
    Attribute {
        subject: AttributeSubject,
        test: Option<(AttributeOperator, String)>,
    },
    Pseudo(Pseudo),
    Not(Negation),
}

#[derive(Debug)]
pub enum AttributeSubject {
    /// `[name]`, `[ns|name]` (as `ns:name`) and `[@name]`
    Name(String),
    /// `[text()]` and the like
    Function(Function),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttributeOperator {
    Equal,
    PrefixMatch,
    SuffixMatch,
    SubstringMatch,
    NotEqual,
    Includes,
    DashMatch,
}

#[derive(Debug)]
pub enum Pseudo {
    Ident(String),
    Function(Function),
}

/// `:not(…)`. The grammar drops the conditions after an element name, so
/// `:not(a.b)` is `:not(a)`.
#[derive(Debug)]
pub enum Negation {
    Element(ElementName),
    Conditions(Vec<Condition>),
}

#[derive(Debug)]
pub struct Function {
    /// The name with its `(`, as the visitor matches it: `nth-child(`.
    pub name: String,
    pub arguments: Vec<Argument>,
}

#[derive(Debug)]
pub enum Argument {
    /// A token as written, escapes and quotes included: the visitor pastes
    /// these into XPath.
    Number(String),
    String(String),
    Ident(String),
    Nth(Nth),
    /// `@name`
    Attribute(String),
    /// The selectors of `has(…)`.
    Selector(Selector),
}

/// `an+b`, as the four strings Nokogiri's `NTH` node holds: `a`, `n`, the
/// operator and `b`.
#[derive(Debug)]
pub struct Nth {
    pub a: String,
    pub operator: String,
    pub b: String,
}

/// What racc keeps on its value stack.
enum Value<'a> {
    Nil,
    Token(&'a str),
    Combinator(Combinator),
    Selectors(Vec<Selector>),
    /// `simple_selector_1toN`, its rest in reverse.
    Chain(Simple, Vec<(Combinator, Simple)>),
    Simple(Simple),
    ElementName(ElementName),
    AttributeName(String),
    Function(Function),
    /// `hcap_1toN`, in reverse.
    Conditions(Vec<Condition>),
    Condition(Condition),
    AttributeTest(AttributeOperator, String),
    Operator(AttributeOperator),
    Arguments(Vec<Argument>),
    Nth(Nth),
    Negation(Negation),
}

/// Parses a selector list, as `Nokogiri::CSS::Parser#parse` does.
pub fn parse(input: &str) -> Result<Vec<Selector>, SyntaxError> {
    let mut tokens = Tokenizer::new(input);
    let mut states: Vec<i16> = vec![0];
    // Values with the source they span, for error messages
    let mut values: Vec<(Value<'_>, &str)> = Vec::new();
    let mut lookahead: Option<Token<'_>> = None;
    let mut read_next = true;
    let mut at_end = false;
    let mut nesting = 0;
    loop {
        let state = *states.last().expect("racc never pops the start state");
        let pointer = t::ACTION_POINTER[index(state)];
        let action = if pointer == N {
            t::ACTION_DEFAULT[index(state)]
        } else {
            if read_next && !at_end {
                lookahead = tokens.next();
                at_end = lookahead.is_none();
                read_next = false;
                match lookahead.map(|token| token.kind) {
                    Some(t::FUNCTION | t::HAS | t::NOT) => nesting += 1,
                    Some(t::RPAREN) => nesting = usize::saturating_sub(nesting, 1),
                    _ => {}
                }
                if nesting > MAX_NESTING {
                    return Err(SyntaxError(format!(
                        "selector nests more than {MAX_NESTING} parentheses deep"
                    )));
                }
            }
            let kind = lookahead.map_or(t::END, |token| token.kind);
            lookup(&t::ACTION_TABLE, &t::ACTION_CHECK, pointer + kind, state)
                .unwrap_or(t::ACTION_DEFAULT[index(state)])
        };

        if action > 0 && action < t::SHIFT_N {
            let text = lookahead.map_or("$", |token| token.text);
            values.push((Value::Token(text), text));
            states.push(action);
            read_next = true;
        } else if action < 0 && action > -t::REDUCE_N {
            let rule = index(-action);
            let (length, lhs) = t::RULES[rule];
            let start = values.len() - usize::from(length);
            let popped: Vec<_> = values.drain(start..).collect();
            states.truncate(states.len() - usize::from(length));
            let span = span(input, popped.iter().map(|(_, text)| *text));
            let value = reduce(rule, popped.into_iter().map(|(value, _)| value).collect())?;
            values.push((value, span));
            let top = *states.last().expect("racc never pops the start state");
            let nonterminal = lhs - t::NT_BASE;
            let pointer = t::GOTO_POINTER[index(nonterminal)];
            let next = (pointer != N)
                .then(|| lookup(&t::GOTO_TABLE, &t::GOTO_CHECK, pointer + top, nonterminal))
                .flatten()
                .unwrap_or(t::GOTO_DEFAULT[index(nonterminal)]);
            states.push(next);
        } else if action == t::SHIFT_N {
            return match values.into_iter().next() {
                Some((Value::Selectors(selectors), _)) => Ok(selectors),
                _ => unreachable!("the start rule reduces to a selector list"),
            };
        } else {
            // Nokogiri's on_error: "unexpected '#{error_value}' after
            // '#{value_stack.compact.last}'"
            let unexpected = lookahead.map_or("$", |token| token.text);
            let after = values
                .iter()
                .rev()
                .find_map(|(value, text)| match value {
                    Value::Nil => None,
                    Value::Combinator(combinator) => Some(combinator.symbol()),
                    _ => Some(*text),
                })
                .unwrap_or("");
            return Err(SyntaxError(format!(
                "unexpected '{unexpected}' after '{after}'"
            )));
        }
    }
}

fn index(value: i16) -> usize {
    usize::try_from(value).expect("racc indexes are positive")
}

/// `table[i]` if `check[i]` says it belongs to `owner`.
fn lookup(table: &[i16], check: &[i16], i: i16, owner: i16) -> Option<i16> {
    let i = usize::try_from(i).ok()?;
    let value = *table.get(i)?;
    (value != N && check.get(i) == Some(&owner)).then_some(value)
}

/// The source text that adjacent slices of `input` cover together.
fn span<'a>(input: &'a str, mut parts: impl Iterator<Item = &'a str>) -> &'a str {
    let offset = |part: &str| part.as_ptr() as usize - input.as_ptr() as usize;
    let parts: Vec<&str> = parts
        .by_ref()
        .filter(|part| !part.is_empty() && *part != "$")
        .collect();
    match (parts.first(), parts.last()) {
        (Some(first), Some(last)) => &input[offset(first)..offset(last) + last.len()],
        _ => "",
    }
}

impl Combinator {
    fn symbol(self) -> &'static str {
        match self {
            Combinator::Child => "CHILD_SELECTOR",
            Combinator::Descendant => "DESCENDANT_SELECTOR",
            Combinator::DirectAdjacent => "DIRECT_ADJACENT_SELECTOR",
            Combinator::Following => "FOLLOWING_SELECTOR",
        }
    }
}

/// The rules `reduce` handles, as racc numbers parser.y's and
/// `tables::GRAMMAR` names them; a test keeps the two equal.
#[cfg(test)]
pub const REDUCED_RULES: [&str; 82] = [
    "",
    "selector: selector COMMA simple_selector_1toN",
    "selector: prefixless_combinator_selector",
    "selector: optional_S simple_selector_1toN",
    "combinator: PLUS",
    "combinator: GREATER",
    "combinator: TILDE",
    "combinator: DOUBLESLASH",
    "combinator: SLASH",
    r#"xpath_attribute_name: "@" IDENT"#,
    "xpath_attribute: xpath_attribute_name",
    "simple_selector: element_name hcap_0toN",
    "simple_selector: function",
    "simple_selector: function pseudo",
    "simple_selector: function attrib",
    "simple_selector: hcap_1toN",
    "simple_selector: xpath_attribute",
    "prefixless_combinator_selector: combinator simple_selector_1toN",
    "simple_selector_1toN: simple_selector combinator simple_selector_1toN",
    "simple_selector_1toN: simple_selector S simple_selector_1toN",
    "simple_selector_1toN: simple_selector",
    r#"class: "." IDENT"#,
    "element_name: namespaced_ident",
    r#"element_name: "*""#,
    r#"namespaced_ident: namespace "|" IDENT"#,
    "namespaced_ident: IDENT",
    "namespace: IDENT",
    "namespace:",
    "attrib: LSQUARE attrib_name attrib_val_0or1 RSQUARE",
    "attrib: LSQUARE function attrib_val_0or1 RSQUARE",
    "attrib: LSQUARE NUMBER RSQUARE",
    r#"attrib_name: namespace "|" IDENT"#,
    "attrib_name: IDENT",
    "attrib_name: xpath_attribute",
    "function: FUNCTION RPAREN",
    "function: FUNCTION expr RPAREN",
    "function: FUNCTION nth RPAREN",
    "function: NOT expr RPAREN",
    "function: HAS selector RPAREN",
    "expr: NUMBER COMMA expr",
    "expr: STRING COMMA expr",
    "expr: IDENT COMMA expr",
    "expr: xpath_attribute COMMA expr",
    "expr: NUMBER",
    "expr: STRING",
    "expr: IDENT",
    "expr: xpath_attribute",
    "nth: NUMBER IDENT PLUS NUMBER",
    "nth: IDENT PLUS NUMBER",
    "nth: NUMBER IDENT",
    r#"pseudo: ":" function"#,
    r#"pseudo: ":" IDENT"#,
    "hcap_0toN: hcap_1toN",
    "hcap_0toN:",
    "hcap_1toN: attribute_id hcap_1toN",
    "hcap_1toN: class hcap_1toN",
    "hcap_1toN: attrib hcap_1toN",
    "hcap_1toN: pseudo hcap_1toN",
    "hcap_1toN: negation hcap_1toN",
    "hcap_1toN: attribute_id",
    "hcap_1toN: class",
    "hcap_1toN: attrib",
    "hcap_1toN: pseudo",
    "hcap_1toN: negation",
    "attribute_id: HASH",
    "attrib_val_0or1: eql_incl_dash IDENT",
    "attrib_val_0or1: eql_incl_dash STRING",
    "attrib_val_0or1: eql_incl_dash NUMBER",
    "attrib_val_0or1:",
    "eql_incl_dash: EQUAL",
    "eql_incl_dash: PREFIXMATCH",
    "eql_incl_dash: SUFFIXMATCH",
    "eql_incl_dash: SUBSTRINGMATCH",
    "eql_incl_dash: NOT_EQUAL",
    "eql_incl_dash: INCLUDES",
    "eql_incl_dash: DASHMATCH",
    "negation: NOT negation_arg RPAREN",
    "negation_arg: element_name",
    "negation_arg: element_name hcap_1toN",
    "negation_arg: hcap_1toN",
    "optional_S: S",
    "optional_S:",
];

/// Runs the action of grammar rule `rule` (numbered as in parser.y) on the
/// values of its right-hand side.
fn reduce(rule: usize, values: Vec<Value<'_>>) -> Result<Value<'_>, SyntaxError> {
    use Value as V;
    let mut values = values.into_iter();
    let mut next = || values.next().expect("racc pops the rule's length");
    Ok(match rule {
        // selector: selector COMMA simple_selector_1toN
        1 => {
            let (V::Selectors(mut selectors), _, chain) = (next(), next(), next()) else {
                unreachable!()
            };
            selectors.push(selector(None, chain));
            V::Selectors(selectors)
        }
        // | prefixless_combinator_selector
        2 => match next() {
            V::Selectors(selectors) => V::Selectors(selectors),
            _ => unreachable!(),
        },
        // | optional_S simple_selector_1toN
        3 => {
            let (_, chain) = (next(), next());
            V::Selectors(vec![selector(None, chain)])
        }
        // combinator: PLUS | GREATER | TILDE | DOUBLESLASH | SLASH
        4 => V::Combinator(Combinator::DirectAdjacent),
        5 | 8 => V::Combinator(Combinator::Child),
        6 => V::Combinator(Combinator::Following),
        7 => V::Combinator(Combinator::Descendant),
        // xpath_attribute_name: '@' IDENT
        9 => {
            next();
            next()
        }
        // xpath_attribute: xpath_attribute_name
        10 => V::AttributeName(token(next()).into()),
        // simple_selector: element_name hcap_0toN
        11 => {
            let (V::ElementName(name), conditions) = (next(), next()) else {
                unreachable!()
            };
            let conditions = match conditions {
                V::Conditions(reversed) => reversed.into_iter().rev().collect(),
                _ => Vec::new(),
            };
            V::Simple(Simple::Element { name, conditions })
        }
        // | function
        12 => V::Simple(Simple::Function {
            function: function(next()),
            condition: None,
        }),
        // | function pseudo | function attrib
        13 | 14 => {
            let function = function(next());
            let V::Condition(condition) = next() else {
                unreachable!()
            };
            V::Simple(Simple::Function {
                function,
                condition: Some(condition),
            })
        }
        // | hcap_1toN
        15 => V::Simple(Simple::Element {
            name: ElementName {
                prefix: None,
                local: "*".into(),
            },
            conditions: conditions(next()),
        }),
        // | xpath_attribute
        16 => match next() {
            V::AttributeName(name) => V::Simple(Simple::Attribute(name)),
            _ => unreachable!(),
        },
        // prefixless_combinator_selector: combinator simple_selector_1toN
        17 => {
            let (V::Combinator(combinator), chain) = (next(), next()) else {
                unreachable!()
            };
            V::Selectors(vec![selector(Some(combinator), chain)])
        }
        // simple_selector_1toN: simple_selector combinator simple_selector_1toN
        //   | simple_selector S simple_selector_1toN
        18 | 19 => {
            let (V::Simple(simple), combinator, chain) = (next(), next(), next()) else {
                unreachable!()
            };
            let combinator = match combinator {
                V::Combinator(combinator) => combinator,
                _ => Combinator::Descendant,
            };
            let V::Chain(head, mut rest) = chain else {
                unreachable!()
            };
            rest.push((combinator, head));
            V::Chain(simple, rest)
        }
        // | simple_selector
        20 => match next() {
            V::Simple(simple) => V::Chain(simple, Vec::new()),
            _ => unreachable!(),
        },
        // class: '.' IDENT
        21 => {
            next();
            V::Condition(Condition::Class(unescape_identifier(token(next()))))
        }
        // element_name: namespaced_ident | '*'
        22 => next(),
        23 => V::ElementName(ElementName {
            prefix: None,
            local: "*".into(),
        }),
        // namespaced_ident: namespace '|' IDENT
        24 => {
            let (namespace, _, local) = (next(), next(), next());
            V::ElementName(ElementName {
                prefix: Some(optional_token(namespace)),
                local: token(local).into(),
            })
        }
        // | IDENT
        25 => V::ElementName(ElementName {
            prefix: None,
            local: token(next()).into(),
        }),
        // namespace: IDENT | (empty)
        26 => next(),
        27 => V::Nil,
        // attrib: LSQUARE attrib_name attrib_val_0or1 RSQUARE
        //   | LSQUARE function attrib_val_0or1 RSQUARE
        28 | 29 => {
            let (_, subject, test, _) = (next(), next(), next(), next());
            let subject = match subject {
                V::AttributeName(name) => AttributeSubject::Name(name),
                function => AttributeSubject::Function(self::function(function)),
            };
            let test = match test {
                V::AttributeTest(operator, value) => Some((operator, value)),
                _ => None,
            };
            V::Condition(Condition::Attribute { subject, test })
        }
        // | LSQUARE NUMBER RSQUARE
        30 => {
            let (_, number, _) = (next(), next(), next());
            V::Condition(Condition::Pseudo(Pseudo::Function(Function {
                name: "nth-child(".into(),
                arguments: vec![Argument::Number(token(number).into())],
            })))
        }
        // attrib_name: namespace '|' IDENT
        31 => {
            let (namespace, _, local) = (next(), next(), next());
            let name = match optional_token(namespace) {
                Some(namespace) => format!("{namespace}:{}", token(local)),
                None => token(local).into(),
            };
            V::AttributeName(name)
        }
        // | IDENT
        32 => V::AttributeName(token(next()).into()),
        // | xpath_attribute
        33 => next(),
        // function: FUNCTION RPAREN
        34 => V::Function(Function {
            name: ruby_strip(token(next())).into(),
            arguments: Vec::new(),
        }),
        // | FUNCTION expr RPAREN | FUNCTION nth RPAREN | NOT expr RPAREN
        //   | HAS selector RPAREN
        35..=38 => {
            let (name, arguments) = (next(), next());
            let arguments = match arguments {
                V::Arguments(arguments) => arguments,
                V::Nth(nth) => vec![Argument::Nth(nth)],
                V::Selectors(selectors) => selectors.into_iter().map(Argument::Selector).collect(),
                _ => unreachable!(),
            };
            V::Function(Function {
                name: ruby_strip(token(name)).into(),
                arguments,
            })
        }
        // expr: NUMBER COMMA expr | STRING COMMA expr | IDENT COMMA expr
        //   | xpath_attribute COMMA expr
        39..=42 => {
            let (first, _, rest) = (next(), next(), next());
            let mut arguments = vec![match (rule, first) {
                (39, first) => Argument::Number(token(first).into()),
                (40, first) => Argument::String(token(first).into()),
                (41, first) => Argument::Ident(token(first).into()),
                (_, V::AttributeName(name)) => Argument::Attribute(name),
                _ => unreachable!(),
            }];
            match rest {
                V::Arguments(rest) => arguments.extend(rest),
                V::Nth(nth) => arguments.push(Argument::Nth(nth)),
                _ => unreachable!(),
            }
            V::Arguments(arguments)
        }
        // | NUMBER | STRING
        43 => V::Arguments(vec![Argument::Number(token(next()).into())]),
        44 => V::Arguments(vec![Argument::String(token(next()).into())]),
        // | IDENT
        45 => {
            let ident = token(next());
            match ident {
                "even" => nth("2", "+", "0"),
                "odd" => nth("2", "+", "1"),
                "n" => nth("1", "+", "0"),
                _ => V::Arguments(vec![Argument::Ident(ident.into())]),
            }
        }
        // | xpath_attribute
        46 => match next() {
            V::AttributeName(name) => V::Arguments(vec![Argument::Attribute(name)]),
            _ => unreachable!(),
        },
        // nth: NUMBER IDENT PLUS NUMBER
        47 => {
            let (a, n, plus, b) = (token(next()), token(next()), token(next()), token(next()));
            if n != "n" {
                return Err(parse_error(n));
            }
            nth(a, plus, b)
        }
        // | IDENT PLUS NUMBER
        48 => {
            let (n, plus, b) = (token(next()), token(next()), token(next()));
            match n {
                "n" => nth("1", plus, b),
                "-n" => nth("-1", plus, b),
                // Nokogiri names the operator here, not the identifier
                _ => return Err(parse_error(plus)),
            }
        }
        // | NUMBER IDENT
        49 => {
            let (a, n) = (token(next()), token(next()));
            match n.strip_prefix("n-") {
                Some(b) => nth(a, "-", b),
                None if n == "n" => nth(a, "+", "0"),
                None => return Err(parse_error(n)),
            }
        }
        // pseudo: ':' function | ':' IDENT
        50 => {
            next();
            V::Condition(Condition::Pseudo(Pseudo::Function(function(next()))))
        }
        51 => {
            next();
            V::Condition(Condition::Pseudo(Pseudo::Ident(token(next()).into())))
        }
        // hcap_0toN: hcap_1toN | (empty)
        52 => next(),
        53 => V::Nil,
        // hcap_1toN: (attribute_id | class | attrib | pseudo | negation) hcap_1toN
        54..=58 => {
            let (V::Condition(condition), V::Conditions(mut reversed)) = (next(), next()) else {
                unreachable!()
            };
            reversed.push(condition);
            V::Conditions(reversed)
        }
        // | attribute_id | class | attrib | pseudo | negation
        59..=63 => match next() {
            V::Condition(condition) => V::Conditions(vec![condition]),
            _ => unreachable!(),
        },
        // attribute_id: HASH
        64 => V::Condition(Condition::Id(unescape_identifier(token(next())))),
        // attrib_val_0or1: eql_incl_dash (IDENT | STRING | NUMBER) | (empty)
        65..=67 => {
            let (V::Operator(operator), value) = (next(), token(next())) else {
                unreachable!()
            };
            let value = match rule {
                65 => unescape_identifier(value),
                66 => unescape_string(value),
                _ => value.into(),
            };
            V::AttributeTest(operator, value)
        }
        68 => V::Nil,
        // eql_incl_dash
        69 => V::Operator(AttributeOperator::Equal),
        70 => V::Operator(AttributeOperator::PrefixMatch),
        71 => V::Operator(AttributeOperator::SuffixMatch),
        72 => V::Operator(AttributeOperator::SubstringMatch),
        73 => V::Operator(AttributeOperator::NotEqual),
        74 => V::Operator(AttributeOperator::Includes),
        75 => V::Operator(AttributeOperator::DashMatch),
        // negation: NOT negation_arg RPAREN
        76 => {
            let (_, V::Negation(negation), _) = (next(), next(), next()) else {
                unreachable!()
            };
            V::Condition(Condition::Not(negation))
        }
        // negation_arg: element_name | element_name hcap_1toN | hcap_1toN
        77 | 78 => match next() {
            V::ElementName(name) => V::Negation(Negation::Element(name)),
            _ => unreachable!(),
        },
        79 => V::Negation(Negation::Conditions(conditions(next()))),
        // optional_S: S | (empty)
        80 => next(),
        81 => V::Nil,
        _ => unreachable!("parser.y has 81 rules"),
    })
}

fn token<'a>(value: Value<'a>) -> &'a str {
    match value {
        Value::Token(text) => text,
        _ => unreachable!("expected a token"),
    }
}

fn optional_token(value: Value<'_>) -> Option<String> {
    match value {
        Value::Token(text) => Some(text.into()),
        _ => None,
    }
}

fn function(value: Value<'_>) -> Function {
    match value {
        Value::Function(function) => function,
        _ => unreachable!("expected a function"),
    }
}

fn conditions(value: Value<'_>) -> Vec<Condition> {
    match value {
        Value::Conditions(reversed) => reversed.into_iter().rev().collect(),
        _ => unreachable!("expected conditions"),
    }
}

fn selector(leading: Option<Combinator>, chain: Value<'_>) -> Selector {
    let Value::Chain(first, mut rest) = chain else {
        unreachable!("expected a chain")
    };
    rest.reverse();
    Selector {
        leading,
        first,
        rest,
    }
}

fn nth<'a>(a: &str, operator: &str, b: &str) -> Value<'a> {
    Value::Nth(Nth {
        a: a.into(),
        operator: operator.into(),
        b: b.into(),
    })
}

/// Racc::ParseError in Nokogiri, which isn't a CSS::SyntaxError.
fn parse_error(text: &str) -> SyntaxError {
    SyntaxError(format!("parse error on IDENT '{text}'"))
}

/// Ruby's `String#strip`: whitespace and NUL.
fn ruby_strip(text: &str) -> &str {
    text.trim_matches(|c: char| c == '\0' || c.is_ascii_whitespace() || c == '\x0b')
}

/// Nokogiri's `unescape_css_identifier`.
pub fn unescape_identifier(text: &str) -> String {
    unescape(text, false)
}

/// Nokogiri's `unescape_css_string`, which also drops escaped newlines.
/// The quotes stay.
pub fn unescape_string(text: &str) -> String {
    unescape(text, true)
}

/// `gsub(/\\(?:([^0-9a-fA-F])|([0-9a-fA-F]{1,6})\s?)/)`: a code point from
/// up to six hex digits and one optional whitespace character, or the
/// escaped character itself.
fn unescape(text: &str, in_string: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(backslash) = rest.find('\\') {
        out.push_str(&rest[..backslash]);
        let after = &rest[backslash + 1..];
        let Some(c) = after.chars().next() else {
            out.push('\\');
            rest = after;
            break;
        };
        if c.is_ascii_hexdigit() {
            let digits = after
                .bytes()
                .take(6)
                .take_while(u8::is_ascii_hexdigit)
                .count();
            let code = u32::from_str_radix(&after[..digits], 16).expect("hex digits");
            // Ruby's pack('U') makes invalid UTF-8 for surrogates and raises
            // past U+10FFFF
            out.push(char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER));
            let tail = &after[digits..];
            let space = tail
                .chars()
                .next()
                .filter(|&c| matches!(c, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r'))
                .map_or(0, char::len_utf8);
            rest = &tail[space..];
        } else {
            if !(in_string && c == '\n') {
                out.push(c);
            }
            rest = &after[c.len_utf8()..];
        }
    }
    out.push_str(rest);
    out
}
