use crate::lex::{Quoted, Token};
use crate::{Error, Kind, Mode, Part};
use std::fmt;
use std::num::NonZeroU32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opt {
    name: String,
    value: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Unchecked(Vec<Opt>);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    mux: bool,
    cap: Option<NonZeroU32>,
    rest: Unchecked,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Typed {
    Mux,
    Cap,
}

impl Typed {
    fn of(name: &str) -> Option<Self> {
        let key: String = name
            .chars()
            .filter(|c| !matches!(c, '-' | '_'))
            .map(|c| c.to_ascii_lowercase())
            .collect();
        match key.as_str() {
            "mux" | "fork" => Some(Self::Mux),
            "maxchildren" => Some(Self::Cap),
            _ => None,
        }
    }
}

fn valid(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

impl Opt {
    pub fn new(name: &str, value: Option<&str>) -> Option<Self> {
        (valid(name) && Typed::of(name).is_none()).then(|| Self {
            name: name.to_ascii_lowercase(),
            value: value.map(Into::into),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
}

impl Unchecked {
    pub fn new(opts: Vec<Opt>) -> Self {
        Self(opts)
    }

    pub fn as_slice(&self) -> &[Opt] {
        &self.0
    }
}

impl Options {
    pub fn new(
        mux: bool,
        cap: Option<NonZeroU32>,
        rest: Unchecked,
    ) -> Option<Self> {
        (mux || cap.is_none()).then_some(Self { mux, cap, rest })
    }

    pub fn mux(&self) -> bool {
        self.mux
    }

    pub fn cap(&self) -> Option<NonZeroU32> {
        self.cap
    }

    pub fn rest(&self) -> &Unchecked {
        &self.rest
    }

    pub(crate) fn parse(
        tokens: &[Token],
        mode: Option<Mode>,
    ) -> Result<Self, Error> {
        let mut out = Self::default();
        let mut cap: Option<&Token> = None;
        let listen = mode == Some(Mode::Listen);
        for tok in tokens {
            if tok.text.is_empty() && tok.eq.is_none() {
                continue;
            }
            let (name, value) = match tok.eq {
                Some(i) => (&tok.text[..i], Some(&tok.text[i + 1..])),
                None => (tok.text.as_str(), None),
            };
            if !valid(name) {
                return Err(tok.fail(Kind::Option(tok.text.clone())));
            }
            let lower = name.to_ascii_lowercase();
            match Typed::of(name) {
                Some(_) if !listen => return Err(tok.fail(Kind::Option(lower))),
                Some(Typed::Mux) => {
                    if value.is_some() {
                        return Err(tok.fail(Kind::Value(lower)));
                    }
                    if out.mux {
                        return Err(tok.fail(Kind::Duplicate("mux".into())));
                    }
                    out.mux = true;
                }
                Some(Typed::Cap) => {
                    if cap.is_some() {
                        return Err(
                            tok.fail(Kind::Duplicate("max-children".into()))
                        );
                    }
                    let bad = || tok.fail(Kind::Value("max-children".into()));
                    let n: u32 = Token::new(value.ok_or_else(bad)?, tok.at)
                        .exact(Part::Port, |_| {
                            Kind::Value("max-children".into())
                        })
                        .map_err(|e| match e.kind {
                            Kind::Missing(_) => bad(),
                            _ => e,
                        })?;
                    out.cap = Some(NonZeroU32::new(n).ok_or_else(bad)?);
                    cap = Some(tok);
                }
                None => out.rest.0.push(Opt {
                    name: lower,
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
        for o in &self.rest.0 {
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
