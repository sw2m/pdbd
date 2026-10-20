use crate::Port;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gram {
    Udp,
    Udplite,
    Ip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Point {
    Port(Port),
    Protocol(u8),
}

impl fmt::Display for Gram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Udp => "UDP",
            Self::Udplite => "UDPLITE",
            Self::Ip => "IP",
        })
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Port(p) => write!(f, "{p}"),
            Self::Protocol(n) => write!(f, "{n}"),
        }
    }
}
