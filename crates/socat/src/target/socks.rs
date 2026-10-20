use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Socks {
    V4,
    V4a,
    V5,
}

impl fmt::Display for Socks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::V4 => "SOCKS4",
            Self::V4a => "SOCKS4A",
            Self::V5 => "SOCKS5",
        })
    }
}
