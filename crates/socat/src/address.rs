use crate::{Error, Mode, Options, Target};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub target: Target,
    pub options: Options,
}

impl Address {
    pub fn parse(s: &str) -> Result<Self, Error> {
        let _ = s;
        todo!()
    }

    pub fn mode(&self) -> Option<Mode> {
        self.target.mode()
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
