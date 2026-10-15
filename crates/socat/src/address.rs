use crate::lex::Lexed;
use crate::{Error, Kind, Mode, Options, Target};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub target: Target,
    pub options: Options,
}

impl Address {
    pub fn parse(s: &str) -> Result<Self, Error> {
        if s.is_empty() {
            return Err(Error::new(0, Kind::Empty));
        }
        let Lexed { params, options } = Lexed::lex(s)?;
        let target = Target::parse(&params)?;
        let options = Options::parse(&options, target.mode())?;
        Ok(Self { target, options })
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.target, self.options)
    }
}

#[cfg(test)]
mod test;
