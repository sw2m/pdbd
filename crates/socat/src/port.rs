use crate::lex::Token;
use crate::{Error, Kind, Part};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Port {
    Number(u16),
    Service(Service),
}

impl Service {
    const MAX: usize = 15; // https://www.rfc-editor.org/rfc/rfc6335#section-5.1

    pub fn new(s: &str) -> Option<Self> {
        let valid = (1..=Self::MAX).contains(&s.len())
            && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && s.bytes().any(|b| b.is_ascii_alphabetic())
            && !s.starts_with('-')
            && !s.ends_with('-')
            && !s.contains("--");
        valid.then(|| Self(s.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Port {
    pub(crate) fn parse(tok: &Token) -> Result<Self, Error> {
        let text = tok.text.as_str();
        if text.is_empty() {
            return Err(tok.fail(Kind::Missing(Part::Port)));
        }
        let bad = || tok.fail(Kind::Port(text.into()));
        let digits = text.strip_prefix('+').unwrap_or(text);
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            return digits.parse().map(Self::Number).map_err(|_| bad());
        }
        Service::new(text).map(Self::Service).ok_or_else(bad)
    }
}

impl fmt::Display for Port {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{n}"),
            Self::Service(s) => f.write_str(s.as_str()),
        }
    }
}
