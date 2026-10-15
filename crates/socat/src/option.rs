use crate::lex::{Quoted, Token};
use crate::{Error, Kind, Mode};
use std::fmt;
use std::num::NonZeroU32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opt {
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub mux: bool,
    pub cap: Option<NonZeroU32>,
    pub rest: Vec<Opt>,
}

impl Options {
    pub(crate) fn parse(
        tokens: &[Token],
        mode: Option<Mode>,
    ) -> Result<Self, Error> {
        let mut out = Self::default();
        let mut cap: Option<&Token> = None;
        let listen = mode == Some(Mode::Listen);
        for tok in tokens {
            let (name, value) = match tok.eq {
                Some(i) => (&tok.text[..i], Some(&tok.text[i + 1..])),
                None => (tok.text.as_str(), None),
            };
            let valid = !name.is_empty()
                && name.bytes().all(|b| {
                    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')
                });
            if !valid {
                return Err(tok.fail(Kind::Option(tok.text.clone())));
            }
            let name = name.to_ascii_lowercase();
            match name.as_str() {
                "mux" | "fork" | "max-children" if !listen => {
                    return Err(tok.fail(Kind::Option(name)));
                }
                "mux" | "fork" => {
                    if value.is_some() {
                        return Err(tok.fail(Kind::Value(name)));
                    }
                    if out.mux {
                        return Err(tok.fail(Kind::Duplicate("mux".into())));
                    }
                    out.mux = true;
                }
                "max-children" => {
                    if cap.is_some() {
                        return Err(tok.fail(Kind::Duplicate(name)));
                    }
                    let digits = value.is_some_and(|v| {
                        !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())
                    });
                    out.cap =
                        value.filter(|_| digits).and_then(|v| v.parse().ok());
                    if out.cap.is_none() {
                        return Err(tok.fail(Kind::Value(name)));
                    }
                    cap = Some(tok);
                }
                _ => out.rest.push(Opt {
                    name,
                    value: value.map(Into::into),
                }),
            }
        }
        match cap {
            Some(tok) if !out.mux => Err(tok.fail(Kind::Cap)),
            _ => Ok(out),
        }
    }
}

impl fmt::Display for Opt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)?;
        match &self.value {
            Some(v) => write!(f, "={}", Quoted(v)),
            None => Ok(()),
        }
    }
}

impl fmt::Display for Options {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for o in &self.rest {
            write!(f, ",{o}")?;
        }
        if self.mux {
            f.write_str(",mux")?;
        }
        match self.cap {
            Some(n) => write!(f, ",max-children={n}"),
            None => Ok(()),
        }
    }
}
