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

    pub fn shift(self, by: usize) -> Self {
        Self {
            at: self.at + by,
            kind: self.kind,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let at = self.at;
        match &self.kind {
            Kind::Empty => write!(f, "at byte {at}: empty address"),
            Kind::Keyword(k) => {
                write!(f, "at byte {at}: unknown address type {k:?}")
            }
            Kind::Missing(what) => write!(f, "at byte {at}: missing {what}"),
            Kind::Extra(p) => {
                write!(f, "at byte {at}: unexpected parameter {p:?}")
            }
            Kind::Host(h) => write!(f, "at byte {at}: invalid host {h:?}"),
            Kind::Port(p) => write!(f, "at byte {at}: invalid port {p:?}"),
            Kind::Cid(c) => write!(f, "at byte {at}: invalid vsock cid {c:?}"),
            Kind::Quote => write!(f, "at byte {at}: unterminated quote"),
            Kind::Escape => write!(f, "at byte {at}: trailing backslash"),
            Kind::Bracket => write!(f, "at byte {at}: unbalanced bracket"),
            Kind::Option(o) => write!(f, "at byte {at}: invalid option {o:?}"),
            Kind::Value(o) => {
                write!(f, "at byte {at}: invalid value for option {o:?}")
            }
        }
    }
}

impl std::error::Error for Error {}
