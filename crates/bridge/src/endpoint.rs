use socat::{Address, Socket};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    Client,
    Daemon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alias {
    Bind,
    Forward,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spec {
    Alias(Alias, Socket),
    Raw(Address),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub side: Side,
    pub spec: Spec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bridge {
    pub client: Spec,
    pub daemon: Spec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Address(socat::Kind),
    Side(String),
    Missing,
    Proto(String),
    Twice(Side),
    Mixed,
    Same(Alias),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub arg: usize,
    pub at: usize,
    pub kind: Kind,
}

impl Side {
    fn name(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::Daemon => "daemon",
        }
    }
}

impl Alias {
    fn name(self) -> &'static str {
        match self {
            Self::Bind => "bind",
            Self::Forward => "forward",
        }
    }
}

impl Error {
    fn new(at: usize, kind: Kind) -> Self {
        Self { arg: 0, at, kind }
    }

    fn address(e: socat::Error, off: usize) -> Self {
        Self::new(off + e.at, Kind::Address(e.kind))
    }
}

impl Spec {
    fn parse(s: &str, off: usize) -> Result<Self, Error> {
        for alias in [Alias::Bind, Alias::Forward] {
            let name = alias.name();
            let Some(after) = s.strip_prefix(name) else {
                continue;
            };
            let after = match after.strip_prefix(':') {
                Some(a) => a,
                None if after.is_empty() => {
                    let e = Kind::Address(socat::Kind::Missing("host"));
                    return Err(Error::new(off + s.len(), e));
                }
                None => continue,
            };
            let off = off + name.len() + 1;
            let first = after.split([':', ',']).next().unwrap_or_default();
            if ["tcp", "udp"].iter().any(|p| first.eq_ignore_ascii_case(p)) {
                return Err(Error::new(off, Kind::Proto(first.to_owned())));
            }
            let socket = Socket::parse(after, socat::Mode::Connect)
                .map_err(|e| Error::address(e, off))?;
            return Ok(Self::Alias(alias, socket));
        }
        Address::parse(s)
            .map(Self::Raw)
            .map_err(|e| Error::address(e, off))
    }

    fn mux(&self) -> bool {
        match self {
            Self::Alias(..) => true,
            Self::Raw(a) => a.mux(),
        }
    }

    fn listen(&self) -> bool {
        match self {
            Self::Alias(a, _) => *a == Alias::Bind,
            Self::Raw(a) => a.mode() == Some(socat::Mode::Listen),
        }
    }
}

impl Endpoint {
    pub fn parse(s: &str) -> Result<Self, Error> {
        let (name, rest) = match s.split_once(':') {
            Some(split) => split,
            None => (s, ""),
        };
        let side = match name {
            "client" => Side::Client,
            "daemon" => Side::Daemon,
            _ => return Err(Error::new(0, Kind::Side(name.to_owned()))),
        };
        let off = (name.len() + 1).min(s.len());
        if rest.is_empty() {
            return Err(Error::new(s.len(), Kind::Missing));
        }
        let spec = Spec::parse(rest, off)?;
        Ok(Self { side, spec })
    }
}

impl Bridge {
    pub fn parse(a: &str, b: &str) -> Result<Self, Error> {
        let a = Endpoint::parse(a)?;
        let b = Endpoint::parse(b).map_err(|e| Error { arg: 1, ..e })?;
        Self::new(a, b)
    }

    pub fn new(a: Endpoint, b: Endpoint) -> Result<Self, Error> {
        let pair = |kind| Error {
            arg: 1,
            at: 0,
            kind,
        };
        if a.side == b.side {
            return Err(pair(Kind::Twice(a.side)));
        }
        match (&a.spec, &b.spec) {
            (Spec::Alias(x, _), Spec::Alias(y, _)) if x == y => {
                return Err(pair(Kind::Same(*x)));
            }
            (Spec::Alias(..), Spec::Raw(_))
            | (Spec::Raw(_), Spec::Alias(..)) => {
                return Err(pair(Kind::Mixed));
            }
            _ => {}
        }
        let (client, daemon) = match a.side {
            Side::Client => (a.spec, b.spec),
            Side::Daemon => (b.spec, a.spec),
        };
        Ok(Self { client, daemon })
    }

    pub fn framed(&self) -> bool {
        self.client.mux() || self.daemon.mux()
    }

    pub fn cap(&self) -> Option<u32> {
        [&self.client, &self.daemon]
            .into_iter()
            .find_map(|s| match s {
                Spec::Raw(a) => a.cap(),
                Spec::Alias(..) => None,
            })
    }

    pub fn ingress(&self) -> Option<Side> {
        match (self.client.listen(), self.daemon.listen()) {
            (true, false) => Some(Side::Client),
            (false, true) => Some(Side::Daemon),
            _ => None,
        }
    }
}

impl FromStr for Endpoint {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        Self::parse(s)
    }
}

impl fmt::Display for Spec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Alias(a, socket) => write!(f, "{}:{socket}", a.name()),
            Self::Raw(a) => write!(f, "{a}"),
        }
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.side.name(), self.spec)
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Address(k) => write!(f, "{k}"),
            Self::Side(s) => {
                write!(f, "side {s:?} is neither client nor daemon")
            }
            Self::Missing => write!(f, "missing endpoint after the side"),
            Self::Proto(p) => write!(
                f,
                "bind/forward carry no proto (got {p:?}); \
                 use a raw TCP-LISTEN:/UDP-LISTEN: address"
            ),
            Self::Twice(s) => {
                write!(f, "both endpoints are on the {} side", s.name())
            }
            Self::Mixed => write!(f, "bind/forward pair only with each other"),
            Self::Same(a) => write!(
                f,
                "two {} ends; a bridge is one bind + one forward",
                a.name()
            ),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "endpoint {}, at byte {}: {}",
            self.arg + 1,
            self.at,
            self.kind
        )
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod test;
