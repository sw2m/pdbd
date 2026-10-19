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
    // https://www.rfc-editor.org/rfc/rfc1123#section-2.1
    pub fn new(s: &str) -> Option<Self> {
        let (bare, root) = s.strip_suffix('.').map_or((s, ""), |b| (b, "."));
        let mapped = Self::map(bare)?;
        let top = mapped.rsplit_once('.').map_or(mapped.as_str(), |(_, t)| t);
        if top.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(Self(format!("{mapped}{root}")))
    }

    // https://www.unicode.org/reports/tr46/#Processing
    #[cfg(feature = "idn")]
    fn map(s: &str) -> Option<String> {
        use idna::uts46::{AsciiDenyList, DnsLength, Hyphens, Uts46};
        let uts46 = Uts46::new();
        let ascii = uts46
            .to_ascii(
                s.as_bytes(),
                AsciiDenyList::STD3,
                Hyphens::CheckFirstLast,
                DnsLength::Verify,
            )
            .ok()?;
        let (unicode, mapped) = uts46.to_unicode(
            ascii.as_bytes(),
            AsciiDenyList::STD3,
            Hyphens::CheckFirstLast,
        );
        mapped.ok().map(|()| unicode.into_owned())
    }

    #[cfg(not(feature = "idn"))]
    fn map(s: &str) -> Option<String> {
        (s.is_ascii() && hostname_validator::is_valid(s))
            .then(|| s.to_ascii_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Zone {
    const MAX: usize = 15; // Linux IFNAMSIZ less its NUL (RFC 9844 §6)

    pub fn new(s: &str) -> Option<Self> {
        let valid = (1..=Self::MAX).contains(&s.len())
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
