//! A small reader for KiCad's s-expression files (`.kicad_sch`, `.kicad_pcb`, `.kicad_sym`, `.kicad_mod`, the
//! library tables). Every node keeps its byte span, so an editor can change one row and leave the rest of the
//! file byte for byte.

use std::fmt;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A bare word or a quoted string, unescaped.
    Atom { text: String, quoted: bool, span: Range<usize> },
    List { items: Vec<Node>, span: Range<usize> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub offset: usize,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

impl Node {
    pub fn span(&self) -> Range<usize> {
        match self {
            Node::Atom { span, .. } | Node::List { span, .. } => span.clone(),
        }
    }

    pub fn as_atom(&self) -> Option<&str> {
        match self {
            Node::Atom { text, .. } => Some(text),
            Node::List { .. } => None,
        }
    }

    pub fn items(&self) -> &[Node] {
        match self {
            Node::List { items, .. } => items,
            Node::Atom { .. } => &[],
        }
    }

    /// The first word of a list: `footprint` for `(footprint "R_0402" ...)`.
    pub fn head(&self) -> Option<&str> {
        self.items().first().and_then(Node::as_atom)
    }

    /// The `i`-th item after the head, as text.
    pub fn arg(&self, i: usize) -> Option<&str> {
        self.items().get(i + 1).and_then(Node::as_atom)
    }

    /// The child lists whose head is `name`.
    pub fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.items().iter().filter(move |n| n.head() == Some(name))
    }

    pub fn child(&self, name: &str) -> Option<&Node> {
        self.items().iter().find(|n| n.head() == Some(name))
    }

    /// The first argument of the child `name`: `uri` of `(lib (name "x") (uri "y"))` is `y`.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.child(name).and_then(|c| c.arg(0))
    }
}

/// Parses one top-level list (what every KiCad file is).
pub fn parse(text: &str) -> Result<Node, ParseError> {
    let mut p = Parser { text, bytes: text.as_bytes(), pos: 0 };
    p.skip_space();
    let node = p.node()?;
    p.skip_space();
    if p.pos < p.bytes.len() {
        return Err(p.error("text after the closing parenthesis"));
    }
    Ok(node)
}

/// Quotes a string the way KiCad writes it.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> ParseError {
        let line = self.bytes[..self.pos.min(self.bytes.len())].iter().filter(|&&b| b == b'\n').count() + 1;
        ParseError { offset: self.pos, line, message: message.to_string() }
    }

    fn skip_space(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn node(&mut self) -> Result<Node, ParseError> {
        match self.bytes.get(self.pos) {
            None => Err(self.error("unexpected end of file")),
            Some(b'(') => self.list(),
            Some(b')') => Err(self.error("unexpected ')'")),
            Some(b'"') => self.string(),
            Some(_) => Ok(self.word()),
        }
    }

    fn list(&mut self) -> Result<Node, ParseError> {
        let start = self.pos;
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_space();
            match self.bytes.get(self.pos) {
                None => return Err(self.error("unclosed '('")),
                Some(b')') => {
                    self.pos += 1;
                    return Ok(Node::List { items, span: start..self.pos });
                }
                Some(_) => items.push(self.node()?),
            }
        }
    }

    fn string(&mut self) -> Result<Node, ParseError> {
        let start = self.pos;
        self.pos += 1;
        let mut text = String::new();
        let mut run = self.pos;
        while let Some(&b) = self.bytes.get(self.pos) {
            match b {
                b'"' => {
                    text.push_str(&self.text[run..self.pos]);
                    self.pos += 1;
                    return Ok(Node::Atom { text, quoted: true, span: start..self.pos });
                }
                b'\\' => {
                    text.push_str(&self.text[run..self.pos]);
                    let Some(&e) = self.bytes.get(self.pos + 1) else { break };
                    match e {
                        b'n' => text.push('\n'),
                        b't' => text.push('\t'),
                        b'r' => text.push('\r'),
                        b'"' | b'\\' => text.push(e as char),
                        // Unknown escapes stay as written; only ASCII is consumed so `run` stays on a char boundary.
                        _ if e.is_ascii() => {
                            text.push('\\');
                            text.push(e as char);
                        }
                        _ => {
                            text.push('\\');
                            self.pos += 1;
                            run = self.pos;
                            continue;
                        }
                    }
                    self.pos += 2;
                    run = self.pos;
                }
                _ => self.pos += 1,
            }
        }
        Err(self.error("unclosed string"))
    }

    fn word(&mut self) -> Node {
        let start = self.pos;
        while let Some(&b) = self.bytes.get(self.pos) {
            if b.is_ascii_whitespace() || b == b'(' || b == b')' || b == b'"' {
                break;
            }
            self.pos += 1;
        }
        Node::Atom { text: self.text[start..self.pos].to_string(), quoted: false, span: start..self.pos }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lists_atoms_and_spans() {
        let text = "(lib (name \"vhrd_x\")(type KiCad) (uri \"${A}/b c\"))";
        let n = parse(text).unwrap();
        assert_eq!(n.head(), Some("lib"));
        assert_eq!(n.value("name"), Some("vhrd_x"));
        assert_eq!(n.value("type"), Some("KiCad"));
        assert_eq!(n.value("uri"), Some("${A}/b c"));
        let uri = n.child("uri").unwrap();
        assert_eq!(&text[uri.span()], "(uri \"${A}/b c\")");
    }

    #[test]
    fn unescapes_strings() {
        let n = parse(r#"(a "q\"uo\\te\n" "ü\x")"#).unwrap();
        assert_eq!(n.arg(0), Some("q\"uo\\te\n"));
        assert_eq!(n.arg(1), Some("ü\\x"));
        assert_eq!(quote("q\"uo\\te\n"), r#""q\"uo\\te\n""#);
    }

    #[test]
    fn reports_errors_with_lines() {
        assert_eq!(parse("(a\n(b").unwrap_err().message, "unclosed '('");
        assert_eq!(parse("(a \"b)").unwrap_err().message, "unclosed string");
        assert_eq!(parse("(a))").unwrap_err().line, 1);
        assert!(parse("").is_err());
    }
}
