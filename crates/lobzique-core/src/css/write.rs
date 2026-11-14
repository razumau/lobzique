//! Nokogiri's `CSS::XPathVisitor` (lib/nokogiri/css/xpath_visitor.rb)
//! writing XPath strings, for `CSS.xpath_for`. [`super::translate`] builds
//! what libxml2 would read from these strings; this pastes tokens as the
//! visitor does, into valid XPath or not.

use super::parser::{
    unescape_string, Argument, AttributeName, AttributeOperator, AttributeSubject, Combinator,
    Condition, ElementName, Function, Negation, Nth, Pseudo, Selector, Simple,
};
use super::translate::{is_of_type, read_a_and_positive_b, to_i, xpath_name};
use super::{SyntaxError, Visitor};

type Result<T> = std::result::Result<T, SyntaxError>;

/// What `"#{node}"` pastes for a `CSS::Node`, where Nokogiri's has the
/// object's address too.
const NODE_TO_S: &str = "#<Nokogiri::CSS::Node>";

pub struct Writer<'a> {
    visitor: &'a Visitor,
}

impl<'a> Writer<'a> {
    pub fn new(visitor: &'a Visitor) -> Writer<'a> {
        Writer { visitor }
    }

    /// `Node#to_xpath`: the prefix, or `.` for a leading `>`, `+` or `~`,
    /// then the selector.
    pub fn selector(&self, selector: &Selector) -> Result<String> {
        let mut xpath = match selector.leading {
            Some(Combinator::Child | Combinator::DirectAdjacent | Combinator::Following) => {
                ".".to_string()
            }
            Some(Combinator::Descendant) | None => self.visitor.prefix.clone(),
        };
        self.chain(&mut xpath, selector)?;
        Ok(xpath)
    }

    fn chain(&self, xpath: &mut String, selector: &Selector) -> Result<()> {
        if let Some(combinator) = selector.leading {
            xpath.push_str(separator(combinator));
        }
        self.simple(xpath, &selector.first)?;
        for (combinator, simple) in &selector.rest {
            xpath.push_str(separator(*combinator));
            self.simple(xpath, simple)?;
        }
        Ok(())
    }

    /// `visit_conditional_selector`, or the bare name, function or attribute.
    fn simple(&self, xpath: &mut String, simple: &Simple) -> Result<()> {
        match simple {
            Simple::Element { name, conditions } => {
                xpath.push_str(&self.element_name(name)?);
                if !conditions.is_empty() {
                    xpath.push('[');
                    self.conditions(xpath, conditions)?;
                    xpath.push(']');
                }
            }
            Simple::Function {
                function,
                condition,
            } => {
                self.function(xpath, function)?;
                if let Some(condition) = condition {
                    xpath.push('[');
                    self.condition(xpath, condition)?;
                    xpath.push(']');
                }
            }
            Simple::Attribute(name) => xpath.push_str(&attribute_name(name)?),
        }
        Ok(())
    }

    /// `visit_combinator`, over the chain the grammar nests to the right:
    /// ` and ` between conditions, `][` before a final `*-of-type`.
    fn conditions(&self, xpath: &mut String, conditions: &[Condition]) -> Result<()> {
        for (index, condition) in conditions.iter().enumerate() {
            if index > 0 {
                let last = index == conditions.len() - 1;
                xpath.push_str(if last && is_of_type(condition) {
                    "]["
                } else {
                    " and "
                });
            }
            self.condition(xpath, condition)?;
        }
        Ok(())
    }

    fn condition(&self, xpath: &mut String, condition: &Condition) -> Result<()> {
        match condition {
            Condition::Id(id) => {
                xpath.push_str("@id=");
                xpath.push_str(&literal(id.strip_prefix('#').unwrap_or(id)));
            }
            Condition::Class(name) => xpath.push_str(&self.css_class("@class", name)),
            Condition::Attribute { subject, test } => {
                self.attribute_condition(xpath, subject, test)?;
            }
            Condition::Pseudo(Pseudo::Ident(name)) => xpath.push_str(&pseudo_class(name)?),
            Condition::Pseudo(Pseudo::Function(function)) => self.function(xpath, function)?,
            Condition::Not(Negation::Element(name)) => {
                xpath.push_str("not(self::");
                xpath.push_str(&self.element_name(name)?);
                xpath.push(')');
            }
            Condition::Not(Negation::Conditions(conditions)) => {
                xpath.push_str("not(");
                self.conditions(xpath, conditions)?;
                xpath.push(')');
            }
        }
        Ok(())
    }

    /// `visit_attribute_condition`
    fn attribute_condition(
        &self,
        xpath: &mut String,
        subject: &AttributeSubject,
        test: &Option<(AttributeOperator, String)>,
    ) -> Result<()> {
        let attribute = match subject {
            AttributeSubject::Name(name) => attribute_name(name)?,
            AttributeSubject::Function(function) => {
                let mut call = String::new();
                self.function(&mut call, function)?;
                call
            }
        };
        let Some((operator, text)) = test else {
            xpath.push_str(&attribute);
            return Ok(());
        };
        let value = literal(text);
        xpath.push_str(&match operator {
            AttributeOperator::Equal => format!("{attribute}={value}"),
            AttributeOperator::NotEqual => format!("{attribute}!={value}"),
            AttributeOperator::SubstringMatch => format!("contains({attribute},{value})"),
            AttributeOperator::PrefixMatch => format!("starts-with({attribute},{value})"),
            AttributeOperator::DashMatch => format!(
                "{attribute}={value} or starts-with({attribute},concat({value},'-'))"
            ),
            AttributeOperator::Includes => self.css_class(&attribute, text),
            AttributeOperator::SuffixMatch => format!(
                "substring({attribute},string-length({attribute})-string-length({value})+1,string-length({value}))={value}"
            ),
        });
        Ok(())
    }

    /// `visit_function`, by the name with its `(`.
    fn function(&self, xpath: &mut String, function: &Function) -> Result<()> {
        let argument = function.arguments.first();
        let nth = match argument {
            Some(Argument::Nth(nth)) => Some(nth),
            _ => None,
        };
        let written = match function.name.as_str() {
            "text(" => "child::text()".into(),
            "self(" => format!("self::{}", pasted(argument)),
            "eq(" => format!("position()={}", pasted(argument)),
            "nth(" | "nth-of-type(" => match nth {
                Some(nth) => self::nth(nth, false, false)?,
                None => format!("position()={}", pasted(argument)),
            },
            "nth-child(" => match nth {
                Some(nth) => self::nth(nth, true, false)?,
                None => format!("count(preceding-sibling::*)={}", to_i(argument)? - 1),
            },
            "nth-last-of-type(" => match nth {
                Some(nth) => self::nth(nth, false, true)?,
                None => match to_i(argument)? - 1 {
                    0 => "position()=last()".into(),
                    index => format!("position()=last()-{index}"),
                },
            },
            "nth-last-child(" => match nth {
                Some(nth) => self::nth(nth, true, true)?,
                None => format!("count(following-sibling::*)={}", to_i(argument)? - 1),
            },
            "first(" | "first-of-type(" => "position()=1".into(),
            "last(" | "last-of-type(" => "position()=last()".into(),
            "contains(" => format!("contains(.,{})", function_argument(argument)?),
            "gt(" => format!("position()>{}", pasted(argument)),
            "only-child(" => "last()=1".into(),
            "comment(" => "comment()".into(),
            "has(" => return self.has(xpath, argument),
            name => {
                validate_function_name(name)?;
                let mut call = format!("nokogiri:{name}.");
                for argument in &function.arguments {
                    call.push(',');
                    call.push_str(&function_argument(Some(argument))?);
                }
                call.push(')');
                call
            }
        };
        xpath.push_str(&written);
        Ok(())
    }

    /// `has(…)`: `.//` and the first selector, or `.` and the selector when
    /// the visitor finds nothing before its combinator. The rest are
    /// ignored.
    fn has(&self, xpath: &mut String, argument: Option<&Argument>) -> Result<()> {
        let Some(Argument::Selector(selector)) = argument else {
            return Err(SyntaxError("has() takes a selector".into()));
        };
        // The visitor checks `value[0].nil?`, which `|name` also passes
        let bare_no_namespace = selector.rest.is_empty()
            && matches!(
                &selector.first,
                Simple::Element { name: ElementName { prefix: Some(None), .. }, conditions }
                    if conditions.is_empty()
            );
        xpath.push('.');
        if selector.leading.is_none() && !bare_no_namespace {
            xpath.push_str("//");
        }
        self.chain(xpath, selector)
    }

    /// `visit_element_name`
    fn element_name(&self, name: &ElementName) -> Result<String> {
        let local = name.local.as_str();
        if self.visitor.html5 && name.prefix.is_none() && local != "*" {
            // HTML5 has namespaces that CSS queries ignore
            return Ok(if !self.visitor.builtins {
                format!("*[local-name()={}]", literal(local))
            } else if xpath_name(local) {
                format!("*:{local}")
            } else {
                format!("*[nokogiri-builtin:local-name-is({})]", literal(local))
            });
        }
        let mut names = name.prefix.iter().flatten().chain([&name.local]);
        if let Some(invalid) = names.find(|n| *n != "*" && !xpath_name(n)) {
            return Err(SyntaxError(format!("Invalid element name '{invalid}'")));
        }
        Ok(match &name.prefix {
            Some(Some(prefix)) => format!("{prefix}:{local}"),
            Some(None) => local.into(),
            None if local != "*" && self.visitor.default_namespace => format!("xmlns:{local}"),
            None => local.into(),
        })
    }

    /// `css_class`: the builtin, or the plain XPath 1.0.
    fn css_class(&self, haystack: &str, needle: &str) -> String {
        if self.visitor.builtins {
            format!("nokogiri-builtin:css-class({haystack},{})", literal(needle))
        } else {
            let padded = literal(&format!(" {needle} "));
            format!("contains(concat(' ',normalize-space({haystack}),' '),{padded})")
        }
    }
}

/// What the visitor's combinator methods put between two selectors.
fn separator(combinator: Combinator) -> &'static str {
    match combinator {
        Combinator::DirectAdjacent => "/following-sibling::*[1]/self::",
        Combinator::Following => "/following-sibling::",
        Combinator::Descendant => "//",
        Combinator::Child => "/",
    }
}

/// `visit_pseudo_class` for a name without parentheses.
fn pseudo_class(name: &str) -> Result<String> {
    Ok(match name {
        "first" | "first-of-type" => "position()=1",
        "first-child" => "count(preceding-sibling::*)=0",
        "last" | "last-of-type" => "position()=last()",
        "last-child" => "count(following-sibling::*)=0",
        "only-child" => "count(preceding-sibling::*)=0 and count(following-sibling::*)=0",
        "only-of-type" => "last()=1",
        "empty" => "not(node())",
        "parent" => "node()",
        "root" => "not(parent::*)",
        _ => {
            validate_function_name(name)?;
            return Ok(format!("nokogiri:{name}(.)"));
        }
    }
    .into())
}

/// `visit_attrib_name`: `@name`, `@prefix:name`, or for a name XPath can't
/// write, a test of `name()`.
fn attribute_name(name: &AttributeName) -> Result<String> {
    let local = name.local.as_str();
    Ok(match &name.prefix {
        Some(prefix) if xpath_name(prefix) && xpath_name(local) => format!("@{prefix}:{local}"),
        Some(prefix) => {
            let message = format!("Invalid attribute name '{prefix}|{local}'");
            return Err(SyntaxError(message));
        }
        None if xpath_name(local) => format!("@{local}"),
        None => format!("@*[namespace-uri()='' and name()={}]", literal(local)),
    })
}

fn validate_function_name(name: &str) -> Result<()> {
    if name.starts_with('-') {
        return Err(SyntaxError(format!("Invalid XPath function name '{name}'")));
    }
    Ok(())
}

/// `nth`: the position along the axis, or among element siblings for the
/// `*-child` forms, against `an+b`.
fn nth(nth: &Nth, child: bool, last: bool) -> Result<String> {
    let (a, b) = read_a_and_positive_b(nth)?;
    let position = match (child, last) {
        (true, false) => "(count(preceding-sibling::*)+1)",
        (true, true) => "(count(following-sibling::*)+1)",
        (false, false) => "position()",
        (false, true) => "(last()-position()+1)",
    };
    Ok(if a == 0 {
        format!("{position}={b}")
    } else if b == 0 && a > 0 {
        format!("({position} mod {a})=0")
    } else {
        let compare = if a < 0 { "<=" } else { ">=" };
        let modulus = a.unsigned_abs();
        if modulus == 1 {
            format!("{position}{compare}{b}")
        } else {
            format!("({position}{compare}{b}) and ((({position}-{b}) mod {modulus})=0)")
        }
    })
}

/// A function's first argument as `"#{node.value[1]}"` pastes it: the token
/// as written, or nothing.
fn pasted(argument: Option<&Argument>) -> &str {
    match argument {
        Some(Argument::Number(token) | Argument::String(token) | Argument::Ident(token)) => token,
        Some(Argument::Nth(_) | Argument::Attribute(_) | Argument::Selector(_)) => NODE_TO_S,
        None => "",
    }
}

/// `xpath_function_argument`: a string's unescaped text as a literal, an
/// `@name`, the rest as written.
fn function_argument(argument: Option<&Argument>) -> Result<String> {
    Ok(match argument {
        Some(Argument::String(string)) => literal(&unescape_string(&string[1..string.len() - 1])),
        Some(Argument::Attribute(name)) => attribute_name(name)?,
        // The visitor has no `visit_nth`
        Some(Argument::Nth(_)) => return Err(SyntaxError("undefined method 'visit_nth'".into())),
        argument => pasted(argument).into(),
    })
}

/// `xpath_string_literal`: XPath strings have no escapes, so one with both
/// quotes is put together with `concat()`.
fn literal(text: &str) -> String {
    if !text.contains('\'') {
        format!("'{text}'")
    } else if !text.contains('"') {
        format!("\"{text}\"")
    } else {
        format!(
            "concat('{}')",
            text.split('\'').collect::<Vec<_>>().join("',\"'\",'")
        )
    }
}
