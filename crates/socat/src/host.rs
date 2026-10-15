use crate::lex::Token;
use crate::{Error, Kind, Part};
use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Host {
    Name(String),
    V4(Ipv4Addr),
    V6(Ipv6Addr),
}

impl Host {
    pub(crate) fn parse(tok: &Token) -> Result<Self, Error> {
        let text = tok.text.as_str();
        if text.is_empty() {
            return Err(tok.fail(Kind::Missing(Part::Host)));
        }
        let bad = || tok.fail(Kind::Host(text.into()));
        if let Some(inner) = text.strip_prefix('[') {
            return inner
                .strip_suffix(']')
                .and_then(|ip| ip.parse().ok())
                .map(Self::V6)
                .ok_or_else(bad);
        }
        if let Ok(ip) = text.parse() {
            return Ok(Self::V4(ip));
        }
        let name = text.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')
        });
        if name {
            Ok(Self::Name(text.to_ascii_lowercase()))
        } else {
            Err(bad())
        }
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(n) => f.write_str(n),
            Self::V4(ip) => write!(f, "{ip}"),
            Self::V6(ip) => write!(f, "[{ip}]"),
        }
    }
}
