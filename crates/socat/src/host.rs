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
    pub fn new(s: &str) -> Option<Self> {
        let _ = s;
        todo!()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Zone {
    pub fn new(s: &str) -> Option<Self> {
        let _ = s;
        todo!()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Host {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}
