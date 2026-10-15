mod lex;

use crate::{Error, Kind};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Connect,
    Listen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Net {
    Tcp,
    Udp,
    Ws,
    Wss,
    Webtransport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Socket {
    pub host: Option<String>,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    File(String),
    Unix {
        mode: Mode,
        path: String,
    },
    Vsock {
        mode: Mode,
        cid: Option<u32>,
        port: u32,
    },
    Net {
        net: Net,
        mode: Mode,
        socket: Socket,
    },
    Exec(String),
    Stdio,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opt {
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub target: Target,
    pub options: Vec<Opt>,
}

impl Address {
    pub fn parse(s: &str) -> Result<Self, Error> {
        let _ = (s, Kind::Empty, lex::Token::default());
        todo!()
    }

    pub fn mode(&self) -> Option<Mode> {
        todo!()
    }

    pub fn mux(&self) -> bool {
        todo!()
    }

    pub fn cap(&self) -> Option<u32> {
        todo!()
    }
}

impl FromStr for Address {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        Self::parse(s)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

#[cfg(test)]
mod test;
