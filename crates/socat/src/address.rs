use crate::lex::Lexed;
use crate::{Error, Kind, Mode, Options, Target};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    target: Target,
    options: Options,
}

impl Address {
    pub fn new(target: Target, options: Options) -> Result<Self, Kind> {
        let typed = options.mux() || options.cap().is_some();
        if typed && target.mode() != Some(Mode::Listen) {
            return Err(Kind::Option("mux".into()));
        }
        Ok(Self { target, options })
    }

    pub fn parse(s: &str) -> Result<Self, Error> {
        if s.is_empty() {
            return Err(Error::new(0, Kind::Empty));
        }
        let Lexed { params, options } = Lexed::lex(s)?;
        let target = Target::parse(&params)?;
        let options = Options::parse(&options, target.mode())?;
        Ok(Self { target, options })
    }

    pub fn target(&self) -> &Target {
        &self.target
    }

    pub fn options(&self) -> &Options {
        &self.options
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
