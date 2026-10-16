use crate::lex::{Params, Quoted, Token};
use crate::{Error, Host, Kind, Part, Port, Resource, Text};
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Web {
    Ws,
    Wss,
    Webtransport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inet {
    Connect { host: Host, port: Port },
    Listen { host: Option<Host>, port: Port },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vsock {
    Connect { cid: u32, port: u32 },
    Listen { port: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    File(Text),
    Open(Text),
    Gopen(Text),
    Unix {
        mode: Mode,
        path: Text,
    },
    Abstract {
        mode: Mode,
        name: Text,
    },
    Vsock(Vsock),
    Net {
        net: Net,
        inet: Inet,
    },
    Web {
        web: Web,
        inet: Inet,
        resource: Resource,
    },
    Exec(Text),
    Stdio,
    Pty,
}

const ROOTS: &[&str] = &[
    "TCP",
    "UDP",
    "WS",
    "WSS",
    "WEBTRANSPORT",
    "UNIX",
    "ABSTRACT",
    "VSOCK",
];

impl Mode {
    fn parse(word: &str) -> Option<Self> {
        match word {
            "CONNECT" | "C" => Some(Self::Connect),
            "LISTEN" | "L" => Some(Self::Listen),
            _ => None,
        }
    }
}

impl Web {
    fn port(self) -> u16 {
        match self {
            Self::Ws => 80,
            Self::Wss | Self::Webtransport => 443,
        }
    }
}

impl Inet {
    fn parse(params: &Params, mode: Mode) -> Result<Self, Error> {
        if mode == Mode::Listen && params.len() <= 1 {
            let port = Port::parse(params.get(0, Part::Port)?)?;
            return Ok(Self::Listen { host: None, port });
        }
        let host = Host::parse(params.get(0, Part::Host)?)?;
        let port = Port::parse(params.get(1, Part::Port)?)?;
        params.done(2)?;
        Ok(match mode {
            Mode::Connect => Self::Connect { host, port },
            Mode::Listen => Self::Listen {
                host: Some(host),
                port,
            },
        })
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
        let port = |i| params.get(i, Part::Port)?.exact(Part::Port, Kind::Port);
        match mode {
            Mode::Connect => {
                let cid =
                    params.get(0, Part::Cid)?.exact(Part::Cid, Kind::Cid)?;
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
            return Ok(Self::Gopen(Params::new(tokens, 0).text(Part::Path)?));
        }
        let params = Params::new(rest, keyword.end);
        let upper = keyword.text.to_ascii_uppercase();
        let url = rest.first().is_some_and(|t| t.text.starts_with("//"));
        Ok(match upper.as_str() {
            "STDIO" | "-" => params.done(0).map(|()| Self::Stdio)?,
            "PTY" => params.done(0).map(|()| Self::Pty)?,
            "FILE" => Self::File(params.text(Part::Path)?),
            "OPEN" => Self::Open(params.text(Part::Path)?),
            "GOPEN" => Self::Gopen(params.text(Part::Path)?),
            "EXEC" => Self::Exec(params.text(Part::Command)?),
            "WS" if url => Self::url(Web::Ws, &params)?,
            "WSS" if url => Self::url(Web::Wss, &params)?,
            "HTTPS" if url => Self::url(Web::Webtransport, &params)?,
            _ => Self::modal(keyword, &upper, &params)?,
        })
    }

    fn modal(
        keyword: &Token,
        upper: &str,
        params: &Params,
    ) -> Result<Self, Error> {
        let unknown = || keyword.fail(Kind::Keyword(keyword.text.clone()));
        let (root, mode) = match upper {
            "TCP" | "UDP" | "UNIX" => (upper, Mode::Connect),
            "LOCAL" => ("UNIX", Mode::Connect),
            _ => {
                let (a, b) = upper.split_once('-').ok_or_else(unknown)?;
                match (Mode::parse(a), Mode::parse(b)) {
                    (_, Some(m)) if ROOTS.contains(&a) => (a, m),
                    (Some(m), _) if ROOTS.contains(&b) => (b, m),
                    _ => return Err(unknown()),
                }
            }
        };
        let inet = || Inet::parse(params, mode);
        Ok(match root {
            "UNIX" => Self::Unix {
                mode,
                path: params.text(Part::Path)?,
            },
            "ABSTRACT" => Self::Abstract {
                mode,
                name: params.text(Part::Name)?,
            },
            "VSOCK" => Self::Vsock(Vsock::parse(params, mode)?),
            "TCP" => Self::Net {
                net: Net::Tcp,
                inet: inet()?,
            },
            "UDP" => Self::Net {
                net: Net::Udp,
                inet: inet()?,
            },
            "WS" => Self::web(Web::Ws, params, mode)?,
            "WSS" => Self::web(Web::Wss, params, mode)?,
            _ => Self::web(Web::Webtransport, params, mode)?,
        })
    }

    fn web(web: Web, params: &Params, mode: Mode) -> Result<Self, Error> {
        let tokens = params.tokens();
        if let [first, _, ..] = tokens {
            if mode == Mode::Listen && first.text.contains('/') {
                return Err(first.fail(Kind::Port(first.text.clone())));
            }
        }
        let bare = mode == Mode::Listen && tokens.len() == 1;
        let k = if bare { 0 } else { 1 };
        let split = tokens.get(k).and_then(|t| Some((t, t.text.find('/')?)));
        let Some((tok, i)) = split else {
            let inet = Inet::parse(params, mode)?;
            return Ok(Self::Web {
                web,
                inet,
                resource: Resource::default(),
            });
        };
        let mut path = tok.text[i..].to_string();
        for t in &tokens[k + 1..] {
            path.push(':');
            path.push_str(&t.text);
        }
        let resource = Resource::parse(&path, tok.at + i)?;
        let mut head = tokens[..k].to_vec();
        head.push(Token::new(&tok.text[..i], tok.at));
        let inet = Inet::parse(&Params::new(&head, params.end()), mode)?;
        Ok(Self::Web {
            web,
            inet,
            resource,
        })
    }

    fn url(web: Web, params: &Params) -> Result<Self, Error> {
        let joined = params.joined();
        let base = params.tokens()[0].at + 2;
        let body = &joined[2..];
        let end = body.find(['/', '?', '#']).unwrap_or(body.len());
        let (authority, path) = body.split_at(end);
        let bad = || Error::new(base, Kind::Host(authority.into()));
        let (host, port) = match authority.strip_prefix('[') {
            Some(_) => {
                let close = authority.find(']').ok_or_else(bad)? + 1;
                match &authority[close..] {
                    "" => (&authority[..close], None),
                    rest => (
                        &authority[..close],
                        Some(rest.strip_prefix(':').ok_or_else(bad)?),
                    ),
                }
            }
            None => match authority.split_once(':') {
                Some((h, p)) => (h, Some(p)),
                None => (authority, None),
            },
        };
        let host = Host::parse(&Token::new(host, base))?;
        let port = match port {
            Some(p) => {
                Port::parse(&Token::new(p, base + authority.len() - p.len()))?
            }
            None => Port::Number(web.port()),
        };
        let at = base + authority.len();
        let resource = match path {
            "" => Resource::default(),
            p if p.starts_with('?') => {
                Resource::parse(&format!("/{p}"), at - 1)?
            }
            p => Resource::parse(p, at)?,
        };
        Ok(Self::Web {
            web,
            inet: Inet::Connect { host, port },
            resource,
        })
    }

    pub fn single(&self) -> bool {
        matches!(
            self,
            Self::File(_)
                | Self::Open(_)
                | Self::Gopen(_)
                | Self::Stdio
                | Self::Pty
        )
    }

    pub fn mode(&self) -> Option<Mode> {
        match self {
            Self::Unix { mode, .. } | Self::Abstract { mode, .. } => {
                Some(*mode)
            }
            Self::Vsock(v) => Some(v.mode()),
            Self::Net { inet, .. } | Self::Web { inet, .. } => {
                Some(inet.mode())
            }
            Self::File(_)
            | Self::Open(_)
            | Self::Gopen(_)
            | Self::Exec(_)
            | Self::Stdio
            | Self::Pty => None,
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
        })
    }
}

impl fmt::Display for Web {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
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
            Self::File(p) => write!(f, "FILE:{}", Quoted(p.as_str())),
            Self::Open(p) => write!(f, "OPEN:{}", Quoted(p.as_str())),
            Self::Gopen(p) => write!(f, "GOPEN:{}", Quoted(p.as_str())),
            Self::Unix { mode, path } => {
                write!(f, "UNIX-{mode}:{}", Quoted(path.as_str()))
            }
            Self::Abstract { mode, name } => {
                write!(f, "ABSTRACT-{mode}:{}", Quoted(name.as_str()))
            }
            Self::Vsock(v) => write!(f, "{v}"),
            Self::Net { net, inet } => {
                write!(f, "{net}-{}:{inet}", inet.mode())
            }
            Self::Web {
                web,
                inet,
                resource,
            } => {
                write!(f, "{web}-{}:{inet}", inet.mode())?;
                if *resource != Resource::default() {
                    write!(f, "{}", Quoted(resource.as_str()))?;
                }
                Ok(())
            }
            Self::Exec(c) => write!(f, "EXEC:{}", Quoted(c.as_str())),
            Self::Stdio => f.write_str("STDIO"),
            Self::Pty => f.write_str("PTY"),
        }
    }
}

#[cfg(test)]
mod test;
