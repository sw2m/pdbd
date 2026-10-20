use crate::{Address, Error, Half, Kind, Nature, Reason, Unlinked};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Channel {
    Single(Address),
    Dual { read: Address, write: Address },
}

impl Channel {
    pub fn parse(s: &str) -> Result<Self, Error> {
        let at = match Address::parse(s) {
            Ok(a) => return Ok(Self::Single(a)),
            Err(Error {
                at,
                kind: Kind::Dual,
            }) => at,
            Err(e) => return Err(e),
        };
        let read = Address::parse(&s[..at])?;
        let rest = at + 2;
        let write = Address::parse(&s[rest..])
            .map_err(|e| Error::new(e.at + rest, e.kind))?;
        Ok(Self::Dual { read, write })
    }

    pub fn link(&self) -> Result<(), Unlinked> {
        match self {
            Self::Single(a) => match a.nature().reason() {
                Some(reason) => Err(Unlinked::new(None, reason)),
                None if a.reads() && a.writes() => Ok(()),
                None => Err(Unlinked::new(None, Reason::Oneway)),
            },
            Self::Dual { read, write } => {
                Self::half(read, Half::Read, read.reads())?;
                Self::half(write, Half::Write, write.writes())
            }
        }
    }

    fn half(a: &Address, half: Half, able: bool) -> Result<(), Unlinked> {
        let fail = |reason| Err(Unlinked::new(Some(half), reason));
        match a.nature() {
            Nature::Peer | Nature::Queue if able => Ok(()),
            Nature::Peer | Nature::Queue => fail(Reason::Oneway),
            n => fail(n.reason().expect("only a peer or a queue has none")),
        }
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Single(a) => write!(f, "{a}"),
            Self::Dual { read, write } => write!(f, "{read}!!{write}"),
        }
    }
}

impl std::str::FromStr for Channel {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        Self::parse(s)
    }
}
