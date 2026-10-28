//! Nokogiri's CSS tokenizer (lib/nokogiri/css/tokenizer.rex), rule for rule:
//! the first rule that matches at the current position wins, as in the
//! generated scanner, not the longest.

use super::tables as t;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token<'a> {
    /// The racc token number, [`t::ERROR`] for a character no rule names.
    pub kind: i16,
    pub text: &'a str,
}

pub struct Tokenizer<'a> {
    input: &'a str,
    position: usize,
}

impl<'a> Tokenizer<'a> {
    pub fn new(input: &'a str) -> Tokenizer<'a> {
        Tokenizer { input, position: 0 }
    }

    fn rest(&self) -> &'a str {
        &self.input[self.position..]
    }
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Token<'a>> {
        let rest = self.rest();
        let first = rest.chars().next()?;
        let (kind, length) = RULES
            .iter()
            .find_map(|(kind, rule)| rule(rest).map(|length| (*kind, length)))
            .unwrap_or_else(|| {
                let kind = match first {
                    '@' => t::AT,
                    '.' => t::DOT,
                    '*' => t::STAR,
                    '|' => t::PIPE,
                    ':' => t::COLON,
                    _ => t::ERROR,
                };
                (kind, first.len_utf8())
            });
        let text = &rest[..length];
        self.position += length;
        Some(Token { kind, text })
    }
}

type Rule = fn(&str) -> Option<usize>;

// `U\+[0-9a-f?]{1,6}…` (UNICODE_RANGE) is left out: IDENT, earlier, always
// takes the `U`
const RULES: [(i16, Rule); 24] = [
    (t::HAS, |s| after(s, "has(").map(|n| n + spaces(&s[n..]))),
    (t::FUNCTION, |s| {
        let n = ident(s)?;
        after(&s[n..], "(").map(|m| n + m + spaces(&s[n + m..]))
    }),
    (t::IDENT, ident),
    (t::HASH, |s| {
        let n = after(s, "#")?;
        let m = repeat(&s[n..], nmchar);
        (m > 0).then_some(n + m)
    }),
    (t::INCLUDES, |s| spaced(s, "~=")),
    (t::DASHMATCH, |s| spaced(s, "|=")),
    (t::PREFIXMATCH, |s| spaced(s, "^=")),
    (t::SUFFIXMATCH, |s| spaced(s, "$=")),
    (t::SUBSTRINGMATCH, |s| spaced(s, "*=")),
    (t::NOT_EQUAL, |s| spaced(s, "!=")),
    (t::EQUAL, |s| spaced(s, "=")),
    (t::RPAREN, |s| {
        let n = spaces(s);
        after(&s[n..], ")").map(|m| n + m)
    }),
    (t::LSQUARE, |s| after(s, "[").map(|n| n + spaces(&s[n..]))),
    (t::RSQUARE, |s| {
        let n = spaces(s);
        after(&s[n..], "]").map(|m| n + m)
    }),
    (t::PLUS, |s| spaced(s, "+")),
    (t::GREATER, |s| spaced(s, ">")),
    (t::COMMA, |s| spaced(s, ",")),
    (t::TILDE, |s| spaced(s, "~")),
    (t::NOT, |s| after(s, ":not(").map(|n| n + spaces(&s[n..]))),
    (t::NUMBER, number),
    (t::DOUBLESLASH, |s| spaced(s, "//")),
    (t::SLASH, |s| spaced(s, "/")),
    (t::S, |s| Some(spaces(s)).filter(|&n| n > 0)),
    (t::STRING, string),
];

/// Ruby's `[\s]`, which includes the vertical tab.
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r')
}

fn spaces(s: &str) -> usize {
    s.len() - s.trim_start_matches(is_space).len()
}

fn after(s: &str, literal: &str) -> Option<usize> {
    s.starts_with(literal).then_some(literal.len())
}

/// `[\s]*literal[\s]*`
fn spaced(s: &str, literal: &str) -> Option<usize> {
    let n = spaces(s);
    let m = after(&s[n..], literal)?;
    Some(n + m + spaces(&s[n + m..]))
}

fn repeat(s: &str, unit: impl Fn(&str) -> Option<usize>) -> usize {
    let mut n = 0;
    while let Some(m) = unit(&s[n..]) {
        n += m;
    }
    n
}

fn first(s: &str) -> Option<char> {
    s.chars().next()
}

/// `{unicode}|\\[^\n\r\f0-9A-Fa-f]`
fn escape(s: &str) -> Option<usize> {
    let rest = s.strip_prefix('\\')?;
    let c = first(rest)?;
    if c.is_ascii_hexdigit() {
        let digits = rest
            .bytes()
            .take(6)
            .take_while(u8::is_ascii_hexdigit)
            .count();
        let tail = &rest[digits..];
        let space = if tail.starts_with("\r\n") {
            2
        } else {
            first(tail)
                .filter(|&c| is_space(c))
                .map_or(0, char::len_utf8)
        };
        Some(1 + digits + space)
    } else if matches!(c, '\n' | '\r' | '\x0c') {
        None
    } else {
        Some(1 + c.len_utf8())
    }
}

/// `[_A-Za-z]|{nonascii}|{escape}`
fn nmstart(s: &str) -> Option<usize> {
    match first(s)? {
        c if c == '_' || c.is_ascii_alphabetic() || !c.is_ascii() => Some(c.len_utf8()),
        _ => escape(s),
    }
}

/// `[_A-Za-z0-9-]|{nonascii}|{escape}`
fn nmchar(s: &str) -> Option<usize> {
    match first(s)? {
        c if c == '-' || c.is_ascii_digit() => Some(1),
        _ => nmstart(s),
    }
}

/// `-?{nmstart}{nmchar}*`
fn ident(s: &str) -> Option<usize> {
    let dash = usize::from(s.starts_with('-'));
    let n = dash + nmstart(&s[dash..])?;
    Some(n + repeat(&s[n..], nmchar))
}

/// `-?([0-9]+|[0-9]*\.[0-9]+)`, whose first alternative wins when it
/// matches at all: `1.5` is the number `1`.
fn number(s: &str) -> Option<usize> {
    let sign = usize::from(s.starts_with('-'));
    let rest = &s[sign..];
    let digits = |s: &str| s.bytes().take_while(u8::is_ascii_digit).count();
    let whole = digits(rest);
    if whole > 0 {
        return Some(sign + whole);
    }
    let fraction = digits(rest.strip_prefix('.')?);
    (fraction > 0).then_some(sign + 1 + fraction)
}

/// `"(?>[^\n\r\f"\\]|\\?{nl}|{nonascii}|{escape})*"` and its single-quoted
/// twin.
fn string(s: &str) -> Option<usize> {
    let quote = first(s).filter(|&c| c == '"' || c == '\'')?;
    let unit = |s: &str| -> Option<usize> {
        let c = first(s)?;
        if !matches!(c, '\n' | '\r' | '\x0c' | '\\') && c != quote {
            return Some(c.len_utf8());
        }
        let backslash = usize::from(c == '\\');
        let rest = &s[backslash..];
        let newline = if rest.starts_with("\r\n") {
            Some(2)
        } else {
            first(rest)
                .filter(|c| matches!(c, '\n' | '\r' | '\x0c'))
                .map(|_| 1)
        };
        newline.map(|n| backslash + n).or_else(|| escape(s))
    };
    let body = 1 + repeat(&s[1..], unit);
    first(&s[body..]).filter(|&c| c == quote).map(|_| body + 1)
}
