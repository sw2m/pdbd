use crate::lex::{Params, Quoted, Token};
use crate::{Error, Host, Kind, Part};
use std::fmt;

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
pub enum Inet {
    Connect { host: Host, port: u16 },
    Listen { host: Option<Host>, port: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vsock {
    Connect { cid: u32, port: u32 },
    Listen { port: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    File(String),
    Unix { mode: Mode, path: String },
    Vsock(Vsock),
    Net { net: Net, inet: Inet },
    Exec(String),
    Stdio,
    Pty,
}

impl Mode {
    fn parse(suffix: &str) -> Option<Self> {
        match suffix {
            "CONNECT" | "C" => Some(Self::Connect),
            "LISTEN" | "L" => Some(Self::Listen),
            _ => None,
        }
    }
}

impl Net {
    fn parse(root: &str) -> Option<Self> {
        match root {
            "TCP" => Some(Self::Tcp),
            "UDP" => Some(Self::Udp),
            "WS" => Some(Self::Ws),
            "WSS" => Some(Self::Wss),
            "WEBTRANSPORT" => Some(Self::Webtransport),
            _ => None,
        }
    }
}

impl Inet {
    fn parse(params: &Params, mode: Mode) -> Result<Self, Error> {
        let port =
            |i| params.get(i, Part::Port)?.number(Part::Port, Kind::Port);
        match mode {
            Mode::Listen if params.len() <= 1 => Ok(Self::Listen {
                host: None,
                port: port(0)?,
            }),
            _ => {
                let host = Host::parse(params.get(0, Part::Host)?)?;
                let port = port(1)?;
                params.done(2)?;
                Ok(match mode {
                    Mode::Connect => Self::Connect { host, port },
                    Mode::Listen => Self::Listen {
                        host: Some(host),
                        port,
                    },
                })
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

impl Vsock {
    fn parse(params: &Params, mode: Mode) -> Result<Self, Error> {
        let port =
            |i| params.get(i, Part::Port)?.number(Part::Port, Kind::Port);
        match mode {
            Mode::Connect => {
                let cid =
                    params.get(0, Part::Cid)?.number(Part::Cid, Kind::Cid)?;
                let port = port(1)?;
                params.done(2)?;
                Ok(Self::Connect { cid, port })
            }
            Mode::Listen => {
                let port = port(0)?;
                params.done(1)?;
                Ok(Self::Listen { port })
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

impl Target {
    pub(crate) fn parse(tokens: &[Token]) -> Result<Self, Error> {
        let (keyword, rest) = tokens.split_first().expect("lex yields a token");
        if keyword.text.contains('/') {
            return Ok(Self::File(Params::new(tokens, 0).rest(Part::Path)?));
        }
        let params = Params::new(rest, keyword.end);
        let upper = keyword.text.to_ascii_uppercase();
        Ok(match upper.as_str() {
            "STDIO" | "-" => params.done(0).map(|()| Self::Stdio)?,
            "PTY" => params.done(0).map(|()| Self::Pty)?,
            "FILE" | "OPEN" | "GOPEN" => Self::File(params.rest(Part::Path)?),
            "EXEC" => Self::Exec(params.rest(Part::Command)?),
            _ => Self::modal(keyword, &upper, &params)?,
        })
    }

    fn modal(
        keyword: &Token,
        upper: &str,
        params: &Params,
    ) -> Result<Self, Error> {
        let unknown = || keyword.fail(Kind::Keyword(keyword.text.clone()));
        let (root, mode) = match upper.split_once('-') {
            Some((root, suffix)) => {
                (root, Mode::parse(suffix).ok_or_else(unknown)?)
            }
            None if matches!(upper, "TCP" | "UDP") => (upper, Mode::Connect),
            None => return Err(unknown()),
        };
        Ok(match root {
            "UNIX" => Self::Unix {
                mode,
                path: params.rest(Part::Path)?,
            },
            "VSOCK" => Self::Vsock(Vsock::parse(params, mode)?),
            _ => Self::Net {
                net: Net::parse(root).ok_or_else(unknown)?,
                inet: Inet::parse(params, mode)?,
            },
        })
    }

    pub fn mode(&self) -> Option<Mode> {
        match self {
            Self::Unix { mode, .. } => Some(*mode),
            Self::Vsock(v) => Some(v.mode()),
            Self::Net { inet, .. } => Some(inet.mode()),
            Self::File(_) | Self::Exec(_) | Self::Stdio | Self::Pty => None,
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Connect => "CONNECT",
            Self::Listen => "LISTEN",
        })
    }
}

impl fmt::Display for Net {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Tcp => "TCP",
            Self::Udp => "UDP",
            Self::Ws => "WS",
            Self::Wss => "WSS",
            Self::Webtransport => "WEBTRANSPORT",
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

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(p) => write!(f, "FILE:{}", Quoted(p)),
            Self::Unix { mode, path } => {
                write!(f, "UNIX-{mode}:{}", Quoted(path))
            }
            Self::Vsock(v) => write!(f, "{v}"),
            Self::Net { net, inet } => {
                write!(f, "{net}-{}:{inet}", inet.mode())
            }
            Self::Exec(c) => write!(f, "EXEC:{}", Quoted(c)),
            Self::Stdio => f.write_str("STDIO"),
            Self::Pty => f.write_str("PTY"),
        }
    }
}
