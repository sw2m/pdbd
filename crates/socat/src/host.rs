use crate::lex::Token;
use crate::{Error, Kind, Part};
use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Host {
    Name(Name),
    V4(Ipv4Addr),
    V6(Ipv6Addr, Option<Zone>),
}

impl Name {
    pub fn new(s: &str) -> Option<Self> {
        let valid = !s.is_empty()
            && s.parse::<Ipv4Addr>().is_err()
            && s.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(c, '.' | '-' | '_')
                    || !(c.is_ascii() || c.is_whitespace() || c.is_control())
            });
        valid.then(|| Self(s.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Zone {
    pub fn new(s: &str) -> Option<Self> {
        let valid = !s.is_empty()
            && s.bytes().all(|b| {
                b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')
            });
        valid.then(|| Self(s.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Host {
    pub(crate) fn parse(tok: &Token) -> Result<Self, Error> {
        let text = tok.text.as_str();
        if text.is_empty() {
            return Err(tok.fail(Kind::Missing(Part::Host)));
        }
        let bad = || tok.fail(Kind::Host(text.into()));
        if let Some(inner) = text.strip_prefix('[') {
            let inner = inner.strip_suffix(']').ok_or_else(bad)?;
            let (ip, zone) = match inner.split_once('%') {
                Some((ip, z)) => (ip, Some(Zone::new(z).ok_or_else(bad)?)),
                None => (inner, None),
            };
            let ip = ip.parse().map_err(|_| bad())?;
            return Ok(Self::V6(ip, zone));
        }
        if let Ok(ip) = text.parse() {
            return Ok(Self::V4(ip));
        }
        Name::new(text).map(Self::Name).ok_or_else(bad)
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(n) => f.write_str(n.as_str()),
            Self::V4(ip) => write!(f, "{ip}"),
            Self::V6(ip, None) => write!(f, "[{ip}]"),
            Self::V6(ip, Some(z)) => write!(f, "[{ip}%{}]", z.as_str()),
        }
    }
}

#[cfg(test)]
mod test;
