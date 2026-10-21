use crate::lex::Params;
use crate::{Error, Host, Mode, Part, Port};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Net {
    Tcp,
    Udp,
    Udplite,
    Sctp,
    Dccp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inet {
    Connect { host: Host, port: Port },
    Listen { host: Option<Host>, port: Port },
}

impl Inet {
    pub(crate) fn parse(params: &Params, mode: Mode) -> Result<Self, Error> {
        if mode == Mode::Listen && params.len() <= 1 {
            let port = Port::parse(params.get(0, Part::Port)?)?;
            return Ok(Self::Listen { host: None, port });
        }
        let host = Host::parse(params.get(0, Part::Host)?)?;
        let port = Port::parse(params.get(1, Part::Port)?)?;
        params.done(2)?;
        Ok(match mode {
            Mode::Listen => Self::Listen {
                host: Some(host),
                port,
            },
            _ => Self::Connect { host, port },
        })
    }

    pub fn mode(&self) -> Mode {
        match self {
            Self::Connect { .. } => Mode::Connect,
            Self::Listen { .. } => Mode::Listen,
        }
    }
}

impl fmt::Display for Net {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Tcp => "TCP",
            Self::Udp => "UDP",
            Self::Udplite => "UDPLITE",
            Self::Sctp => "SCTP",
            Self::Dccp => "DCCP",
        })
    }
}

impl fmt::Display for Inet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect { host, port }
            | Self::Listen {
                host: Some(host),
                port,
            } => write!(f, "{host}:{port}"),
            Self::Listen { host: None, port } => write!(f, "{port}"),
        }
    }
}
