use crate::lex::Lexed;
use crate::option::Allow;
use crate::{Access, Error, Kind, Mode, Nature, Options, Target};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    target: Target,
    options: Options,
}

impl Address {
    pub fn new(target: Target, options: Options) -> Result<Self, Kind> {
        target.check()?;
        let typed = options.mux() || options.cap().is_some();
        if typed && !target.forks() {
            return Err(Kind::Option("mux".into()));
        }
        if let Some(a) = options.access().filter(|_| !target.access()) {
            return Err(Kind::Option(a.to_string()));
        }
        Ok(Self { target, options })
    }

    pub fn parse(s: &str) -> Result<Self, Error> {
        if s.is_empty() {
            return Err(Error::new(0, Kind::Empty));
        }
        let Lexed { params, options } = Lexed::lex(s)?;
        let target = Target::parse(&params)?;
        let allow = Allow {
            fork: target.forks(),
            access: target.access(),
        };
        let options = Options::parse(&options, allow)?;
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

    pub(crate) fn nature(&self) -> Nature {
        self.target.nature()
    }

    pub(crate) fn reads(&self) -> bool {
        self.target.reads() && self.options.access() != Some(Access::Write)
    }

    pub(crate) fn writes(&self) -> bool {
        self.target.writes() && self.options.access() != Some(Access::Read)
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
