use crate::{Error, Kind, Part};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) text: String,
    pub(crate) at: usize,
    pub(crate) end: usize,
    pub(crate) eq: Option<usize>,
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
    pub(crate) fn fail(&self, kind: Kind) -> Error {
        Error::new(self.at, kind)
    }

    pub(crate) fn number<T: FromStr>(
        &self,
        part: Part,
        kind: fn(String) -> Kind,
    ) -> Result<T, Error> {
        if self.text.is_empty() {
            return Err(self.fail(Kind::Missing(part)));
        }
        let digits = self.text.bytes().all(|b| b.is_ascii_digit());
        match self.text.parse() {
            Ok(n) if digits => Ok(n),
            _ => Err(self.fail(kind(self.text.clone()))),
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

    pub(crate) fn get(&self, i: usize, part: Part) -> Result<&'a Token, Error> {
        self.tokens
            .get(i)
            .ok_or(Error::new(self.end, Kind::Missing(part)))
    }

    pub(crate) fn done(&self, n: usize) -> Result<(), Error> {
        match self.tokens.get(n) {
            Some(t) => Err(t.fail(Kind::Extra(t.text.clone()))),
            None => Ok(()),
        }
    }

    pub(crate) fn rest(&self, part: Part) -> Result<String, Error> {
        let joined = self
            .tokens
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join(":");
        if joined.is_empty() {
            let at = self.tokens.first().map_or(self.end, |t| t.at);
            return Err(Error::new(at, Kind::Missing(part)));
        }
        Ok(joined)
    }
}

fn closer(open: char) -> char {
    match open {
        '[' => ']',
        '(' => ')',
        _ => '}',
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
        let mut chars = s.char_indices();

        while let Some((i, c)) = chars.next() {
            match c {
                ':' if stack.is_empty() && !options => {
                    out.push(&mut tok, i, false)
                }
                ',' if stack.is_empty() => {
                    out.push(&mut tok, i, options);
                    options = true;
                }
                '=' if stack.is_empty() && options && tok.eq.is_none() => {
                    tok.eq = Some(tok.text.len());
                    tok.text.push(c);
                }
                '\\' => match chars.next() {
                    Some((_, e)) => tok.text.push(e),
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
                        Some((_, '\\')) => match chars.next() {
                            Some((_, e)) => tok.text.push(e),
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
                ']' | ')' | '}' => match stack.pop() {
                    Some((open, _)) if closer(open) == c => tok.text.push(c),
                    _ => return Err(Error::new(i, Kind::Bracket)),
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
