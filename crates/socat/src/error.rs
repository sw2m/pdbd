use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Host,
    Port,
    Path,
    Command,
    Cid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Empty,
    Keyword(String),
    Missing(Part),
    Extra(String),
    Host(String),
    Port(String),
    Cid(String),
    Quote,
    Escape,
    Bracket,
    Option(String),
    Value(String),
    Duplicate(String),
    Cap,
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

impl fmt::Display for Part {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Host => "host",
            Self::Port => "port",
            Self::Path => "path",
            Self::Command => "command",
            Self::Cid => "vsock cid",
        })
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty address"),
            Self::Keyword(k) => write!(f, "unknown address type {k:?}"),
            Self::Missing(part) => write!(f, "missing {part}"),
            Self::Extra(p) => write!(f, "unexpected parameter {p:?}"),
            Self::Host(h) => write!(f, "invalid host {h:?}"),
            Self::Port(p) => write!(f, "invalid port {p:?}"),
            Self::Cid(c) => write!(f, "invalid vsock cid {c:?}"),
            Self::Quote => write!(f, "unterminated quote"),
            Self::Escape => write!(f, "trailing backslash"),
            Self::Bracket => write!(f, "unbalanced bracket"),
            Self::Option(o) => write!(f, "invalid option {o:?}"),
            Self::Value(o) => write!(f, "invalid value for option {o:?}"),
            Self::Duplicate(o) => write!(f, "duplicate option {o:?}"),
            Self::Cap => write!(f, "max-children without mux"),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at byte {}: {}", self.at, self.kind)
    }
}

impl std::error::Error for Error {}
