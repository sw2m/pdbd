use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Host {
    Name(String),
    V4(Ipv4Addr),
    V6(Ipv6Addr),
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
