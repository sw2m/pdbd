use crate::{Error, Kind, Part, Text};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) text: String,
    pub(crate) at: usize,
    pub(crate) end: usize,
    pub(crate) eq: Option<usize>,
    pub(crate) cooked: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Lexed {
    pub(crate) params: Vec<Token>,
    pub(crate) options: Vec<Token>,
}

pub(crate) struct Params<'a> {
    tokens: &'a [Token],
    end: usize,
}

pub(crate) struct Quoted<'a>(pub(crate) &'a str);

const SPECIAL: &[char] =
    &[':', ',', '\'', '"', '\\', '[', ']', '(', ')', '{', '}'];

impl Token {
    pub(crate) fn new(text: &str, at: usize) -> Self {
        Self {
            text: text.into(),
            at,
            end: at + text.len(),
            eq: None,
            cooked: false,
        }
    }

    pub(crate) fn fail(&self, kind: Kind) -> Error {
        Error::new(self.at, kind)
    }

    pub(crate) fn exact<T: FromStr>(
        &self,
        part: Part,
        kind: fn(String) -> Kind,
    ) -> Result<T, Error> {
        let text = self.text.as_str();
        if text.is_empty() {
            return Err(self.fail(Kind::Missing(part)));
        }
        let digits = text.bytes().all(|b| b.is_ascii_digit());
        let radix = text.bytes().all(|b| b.is_ascii_alphanumeric());
        if text.len() > 1 && text.starts_with('0') && radix {
            return Err(self.fail(Kind::Base(text.into())));
        }
        match text.parse() {
            Ok(n) if digits => Ok(n),
            _ => Err(self.fail(kind(text.into()))),
        }
    }
}

impl<'a> Params<'a> {
    pub(crate) fn new(tokens: &'a [Token], end: usize) -> Self {
        let end = tokens.last().map_or(end, |t| t.end);
        Self { tokens, end }
    }

    pub(crate) fn len(&self) -> usize {
        self.tokens.len()
    }

    pub(crate) fn tokens(&self) -> &'a [Token] {
        self.tokens
    }

    pub(crate) fn end(&self) -> usize {
        self.end
    }

    pub(crate) fn get(&self, i: usize, part: Part) -> Result<&'a Token, Error> {
        self.tokens
            .get(i)
            .ok_or(Error::new(self.end, Kind::Missing(part)))
    }

    pub(crate) fn done(&self, n: usize) -> Result<(), Error> {
        match self.tokens.get(n) {
            Some(t) if t.text.is_empty() => Err(t.fail(Kind::Blank)),
            Some(t) => Err(t.fail(Kind::Extra(t.text.clone()))),
            None => Ok(()),
        }
    }

    pub(crate) fn joined(&self) -> String {
        self.tokens
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join(":")
    }

    pub(crate) fn number<T: FromStr>(
        &self,
        i: usize,
        part: Part,
    ) -> Result<T, Error> {
        self.get(i, part)?.exact(part, Kind::Number)
    }

    pub(crate) fn optional(&self, part: Part) -> Result<Option<Text>, Error> {
        match self.tokens.first() {
            None => Ok(None),
            Some(t) if self.joined().is_empty() => Err(t.fail(Kind::Blank)),
            Some(_) => self.text(part).map(Some),
        }
    }

    pub(crate) fn text(&self, part: Part) -> Result<Text, Error> {
        Text::new(self.joined()).ok_or_else(|| {
            let at = self.tokens.first().map_or(self.end, |t| t.at);
            Error::new(at, Kind::Missing(part))
        })
    }
}

fn closer(open: char) -> char {
    match open {
        '[' => ']',
        '(' => ')',
        _ => '}',
    }
}

fn escape(c: char) -> char {
    match c {
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        'f' => '\u{c}',
        'b' => '\u{8}',
        'a' => '\u{7}',
        'v' => '\u{b}',
        c => c,
    }
}

impl Lexed {
    fn push(&mut self, tok: &mut Token, at: usize, options: bool) {
        tok.end = at;
        let next = Token {
            at: at + 1,
            ..Token::default()
        };
        let done = std::mem::replace(tok, next);
        if options {
            self.options.push(done);
        } else {
            self.params.push(done);
        }
    }

    pub(crate) fn lex(s: &str) -> Result<Self, Error> {
        let mut out = Self::default();
        let mut options = false;
        let mut tok = Token::default();
        let mut stack: Vec<(char, usize)> = Vec::new();
        let mut chars = s.char_indices().peekable();

        while let Some((i, c)) = chars.next() {
            let top = stack.is_empty();
            tok.cooked |= matches!(c, '\\' | '\'' | '"');
            match c {
                ':' if top && !options => out.push(&mut tok, i, false),
                ',' if top => {
                    out.push(&mut tok, i, options);
                    options = true;
                }
                '=' if top && options && tok.eq.is_none() => {
                    tok.eq = Some(tok.text.len());
                    tok.text.push(c);
                }
                '!' if top && matches!(chars.peek(), Some((_, '!'))) => {
                    return Err(Error::new(i, Kind::Dual));
                }
                '\\' => match chars.next() {
                    Some((_, '0')) => return Err(Error::new(i, Kind::Nul)),
                    Some((_, e)) => tok.text.push(escape(e)),
                    None => return Err(Error::new(i, Kind::Escape)),
                },
                '\'' => loop {
                    match chars.next() {
                        Some((_, '\'')) => break,
                        Some((_, q)) => tok.text.push(q),
                        None => return Err(Error::new(i, Kind::Quote)),
                    }
                },
                '"' => loop {
                    match chars.next() {
                        Some((_, '"')) => break,
                        Some((j, '\\')) => match chars.next() {
                            Some((_, '0')) => {
                                return Err(Error::new(j, Kind::Nul))
                            }
                            Some((_, e)) => tok.text.push(escape(e)),
                            None => return Err(Error::new(i, Kind::Quote)),
                        },
                        Some((_, q)) => tok.text.push(q),
                        None => return Err(Error::new(i, Kind::Quote)),
                    }
                },
                '[' | '(' | '{' => {
                    stack.push((c, i));
                    tok.text.push(c);
                }
                ']' | ')' | '}' => match stack.last() {
                    None => tok.text.push(c),
                    Some(&(open, _)) if closer(open) == c => {
                        stack.pop();
                        tok.text.push(c);
                    }
                    Some(_) => return Err(Error::new(i, Kind::Bracket)),
                },
                _ => tok.text.push(c),
            }
        }
        if let Some((_, at)) = stack.last() {
            return Err(Error::new(*at, Kind::Bracket));
        }
        out.push(&mut tok, s.len(), options);
        Ok(out)
    }
}

impl fmt::Display for Quoted<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plain = !self.0.is_empty()
            && !self.0.contains("!!")
            && !self.0.chars().any(|c| {
                SPECIAL.contains(&c) || c.is_whitespace() || c.is_control()
            });
        if plain {
            f.write_str(self.0)
        } else {
            write!(f, "'{}'", self.0.replace('\'', r"'\''"))
        }
    }
}
