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

impl Endpoint {
    pub fn parse(s: &str) -> Result<Self, Error> {
        let _ = s;
        todo!()
    }
}

impl Bridge {
    pub fn parse(a: &str, b: &str) -> Result<Self, Error> {
        let _ = (a, b);
        todo!()
    }

    pub fn new(a: Endpoint, b: Endpoint) -> Result<Self, Error> {
        let _ = (a, b);
        todo!()
    }

    pub fn framed(&self) -> bool {
        todo!()
    }

    pub fn cap(&self) -> Option<u32> {
        todo!()
    }

    pub fn ingress(&self) -> Option<Side> {
        todo!()
    }
}

impl FromStr for Endpoint {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        Self::parse(s)
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod test;
