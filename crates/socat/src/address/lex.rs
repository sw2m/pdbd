use crate::{Error, Kind};

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

const SPECIAL: &[char] =
    &[':', ',', '\'', '"', '\\', '[', ']', '(', ')', '{', '}', '='];

pub(crate) fn quote(s: &str) -> String {
    let plain = !s.is_empty()
        && !s.chars().any(|c| {
            SPECIAL.contains(&c) || c.is_whitespace() || c.is_control()
        });
    if plain {
        s.to_owned()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
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
        if let Some((_, at)) = stack.first() {
            return Err(Error::new(*at, Kind::Bracket));
        }
        out.push(&mut tok, s.len(), options);
        Ok(out)
    }
}
