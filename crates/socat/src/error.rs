use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Empty,
    Keyword(String),
    Missing(&'static str),
    Extra(String),
    Host(String),
    Port(String),
    Cid(String),
    Quote,
    Escape,
    Bracket,
    Option(String),
    Value(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub at: usize,
    pub kind: Kind,
}

impl Error {
    pub fn new(at: usize, kind: Kind) -> Self {
        Self { at, kind }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty address"),
            Self::Keyword(k) => write!(f, "unknown address type {k:?}"),
            Self::Missing(what) => write!(f, "missing {what}"),
            Self::Extra(p) => write!(f, "unexpected parameter {p:?}"),
            Self::Host(h) => write!(f, "invalid host {h:?}"),
            Self::Port(p) => write!(f, "invalid port {p:?}"),
            Self::Cid(c) => write!(f, "invalid vsock cid {c:?}"),
            Self::Quote => write!(f, "unterminated quote"),
            Self::Escape => write!(f, "trailing backslash"),
            Self::Bracket => write!(f, "unbalanced bracket"),
            Self::Option(o) => write!(f, "invalid option {o:?}"),
            Self::Value(o) => write!(f, "invalid value for option {o:?}"),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at byte {}: {}", self.at, self.kind)
    }
}

impl std::error::Error for Error {}
