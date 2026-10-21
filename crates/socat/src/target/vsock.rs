use crate::lex::Params;
use crate::{Error, Kind, Mode, Part};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vsock {
    Connect { cid: u32, port: u32 },
    Listen { port: u32 },
}

impl Vsock {
    pub(crate) fn parse(params: &Params, mode: Mode) -> Result<Self, Error> {
        let port = |i| params.get(i, Part::Port)?.exact(Part::Port, Kind::Port);
        match mode {
            Mode::Listen => {
                let port = port(0)?;
                params.done(1)?;
                Ok(Self::Listen { port })
            }
            _ => {
                let cid =
                    params.get(0, Part::Cid)?.exact(Part::Cid, Kind::Cid)?;
                let port = port(1)?;
                params.done(2)?;
                Ok(Self::Connect { cid, port })
            }
        }
    }

    pub fn mode(&self) -> Mode {
        match self {
            Self::Connect { .. } => Mode::Connect,
            Self::Listen { .. } => Mode::Listen,
        }
    }
}

impl fmt::Display for Vsock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect { cid, port } => {
                write!(f, "VSOCK-CONNECT:{cid}:{port}")
            }
            Self::Listen { port } => write!(f, "VSOCK-LISTEN:{port}"),
        }
    }
}
