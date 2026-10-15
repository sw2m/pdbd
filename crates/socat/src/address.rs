mod lex;

use crate::{Error, Kind};
use lex::{quote, Lexed, Token};
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

const NETS: &[(&str, Net, bool)] = &[
    ("TCP", Net::Tcp, true), // socat: bare TCP / UDP connect
    ("UDP", Net::Udp, true),
    ("WS", Net::Ws, false),
    ("WSS", Net::Wss, false),
    ("WEBTRANSPORT", Net::Webtransport, false),
];

impl Mode {
    fn keyword(s: &str) -> Option<Self> {
        match s {
            "CONNECT" | "C" => Some(Self::Connect),
            "LISTEN" | "L" => Some(Self::Listen),
            _ => None,
        }
    }
}

impl Net {
    fn keyword(s: &str) -> Option<(Self, Mode)> {
        NETS.iter().find_map(|(name, net, bare)| {
            if s == *name {
                return bare.then_some((*net, Mode::Connect));
            }
            let mode = Mode::keyword(s.strip_prefix(name)?.strip_prefix('-')?)?;
            Some((*net, mode))
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Tcp => "TCP",
            Self::Udp => "UDP",
            Self::Ws => "WS",
            Self::Wss => "WSS",
            Self::Webtransport => "WEBTRANSPORT",
        }
    }
}

fn number<T: FromStr>(
    tok: &Token,
    kind: fn(String) -> Kind,
) -> Result<T, Error> {
    let text = &tok.text;
    let digits = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    match text.parse() {
        Ok(n) if digits => Ok(n),
        _ => Err(Error::new(tok.at, kind(text.clone()))),
    }
}

fn hostish(s: &str, colon: bool) -> bool {
    !s.is_empty()
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '.' | '-' | '_' | '%')
                || (colon && c == ':')
        })
}

fn host(tok: &Token) -> Result<String, Error> {
    let text = &tok.text;
    if text.is_empty() {
        return Err(Error::new(tok.at, Kind::Missing("host")));
    }
    let (inner, colon) = match text.strip_prefix('[') {
        Some(rest) => match rest.strip_suffix(']') {
            Some(inner) => (inner, true),
            None => return Err(Error::new(tok.at, Kind::Bracket)),
        },
        None => (text.as_str(), false),
    };
    if hostish(inner, colon) {
        Ok(inner.to_owned())
    } else {
        Err(Error::new(tok.at, Kind::Host(text.clone())))
    }
}

fn extra(params: &[Token], n: usize) -> Result<(), Error> {
    match params.get(n) {
        Some(t) => Err(Error::new(t.at, Kind::Extra(t.text.clone()))),
        None => Ok(()),
    }
}

fn rest(
    params: &[Token],
    end: usize,
    what: &'static str,
) -> Result<String, Error> {
    let joined = params
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(":");
    if joined.is_empty() {
        let at = params.first().map_or(end, |t| t.at);
        return Err(Error::new(at, Kind::Missing(what)));
    }
    Ok(joined)
}

impl Socket {
    pub fn parse(s: &str, mode: Mode) -> Result<Self, Error> {
        let lexed = Lexed::lex(s)?;
        if let Some(t) = lexed.options.first() {
            return Err(Error::new(t.at, Kind::Option(t.text.clone())));
        }
        let params = &lexed.params;
        if params.len() == 1 && params[0].text.is_empty() {
            let what = match mode {
                Mode::Connect => "host",
                Mode::Listen => "port",
            };
            return Err(Error::new(0, Kind::Missing(what)));
        }
        Self::from(params, mode, 0)
    }

    fn from(params: &[Token], mode: Mode, end: usize) -> Result<Self, Error> {
        let end = params.last().map_or(end, |t| t.end);
        match (mode, params) {
            (Mode::Connect, []) => Err(Error::new(end, Kind::Missing("host"))),
            (Mode::Connect, [h]) => {
                host(h)?;
                Err(Error::new(end, Kind::Missing("port")))
            }
            (Mode::Listen, []) => Err(Error::new(end, Kind::Missing("port"))),
            (Mode::Listen, [p]) => Ok(Self {
                host: None,
                port: number(p, Kind::Port)?,
            }),
            (_, [h, p, ..]) => {
                let host = host(h)?;
                let port = number(p, Kind::Port)?;
                extra(params, 2)?;
                Ok(Self {
                    host: Some(host),
                    port,
                })
            }
        }
    }
}

impl fmt::Display for Socket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.host {
            Some(h) if h.contains(':') => write!(f, "[{h}]:{}", self.port),
            Some(h) => write!(f, "{h}:{}", self.port),
            None => write!(f, "{}", self.port),
        }
    }
}

impl Opt {
    fn from(tok: &Token, mode: Option<Mode>) -> Result<Self, Error> {
        let fail = |kind| Err(Error::new(tok.at, kind));
        let (name, value) = match tok.eq {
            Some(i) => (&tok.text[..i], Some(tok.text[i + 1..].to_owned())),
            None => (tok.text.as_str(), None),
        };
        if name.is_empty() {
            return fail(Kind::Option(tok.text.clone()));
        }
        let name = name.to_lowercase();
        let listen = mode == Some(Mode::Listen);
        match name.as_str() {
            "mux" | "fork" | "max-children" if !listen => {
                return fail(Kind::Option(name));
            }
            "mux" | "fork" if value.is_some() => {
                return fail(Kind::Value(name))
            }
            "max-children" => {
                let ok = value.as_ref().is_some_and(|v| {
                    !v.is_empty()
                        && v.bytes().all(|b| b.is_ascii_digit())
                        && v.parse::<u32>().is_ok()
                });
                if !ok {
                    return fail(Kind::Value(name));
                }
            }
            _ => {}
        }
        Ok(Self { name, value })
    }
}

impl fmt::Display for Opt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", quote(&self.name))?;
        match &self.value {
            Some(v) => write!(f, "={}", quote(v)),
            None => Ok(()),
        }
    }
}

impl Target {
    fn from(
        keyword: &Token,
        params: &[Token],
        path: bool,
    ) -> Result<Self, Error> {
        let end = keyword.end;
        if path {
            let mut all = vec![keyword.clone()];
            all.extend_from_slice(params);
            return Ok(Self::File(rest(&all, end, "path")?));
        }
        let upper = keyword.text.to_uppercase();
        if let Some((net, mode)) = Net::keyword(&upper) {
            let socket = Socket::from(params, mode, end)?;
            return Ok(Self::Net { net, mode, socket });
        }
        let target = match upper.as_str() {
            "STDIO" | "-" => {
                extra(params, 0)?;
                Self::Stdio
            }
            "FILE" | "OPEN" | "GOPEN" => Self::File(rest(params, end, "path")?),
            "EXEC" => Self::Exec(rest(params, end, "command")?),
            "UNIX-CONNECT" | "UNIX-LISTEN" | "UNIX-L" => Self::Unix {
                mode: if upper == "UNIX-CONNECT" {
                    Mode::Connect
                } else {
                    Mode::Listen
                },
                path: rest(params, end, "path")?,
            },
            "VSOCK-CONNECT" => {
                let end = params.last().map_or(end, |t| t.end);
                let cid = params
                    .first()
                    .ok_or(Error::new(end, Kind::Missing("cid")))?;
                let port = params
                    .get(1)
                    .ok_or(Error::new(end, Kind::Missing("port")))?;
                let cid = number(cid, Kind::Cid)?;
                let port = number(port, Kind::Port)?;
                extra(params, 2)?;
                Self::Vsock {
                    mode: Mode::Connect,
                    cid: Some(cid),
                    port,
                }
            }
            "VSOCK-LISTEN" | "VSOCK-L" => {
                let port = params
                    .first()
                    .ok_or(Error::new(end, Kind::Missing("port")))?;
                let port = number(port, Kind::Port)?;
                extra(params, 1)?;
                Self::Vsock {
                    mode: Mode::Listen,
                    cid: None,
                    port,
                }
            }
            _ => {
                return Err(Error::new(
                    keyword.at,
                    Kind::Keyword(keyword.text.clone()),
                ));
            }
        };
        Ok(target)
    }

    fn mode(&self) -> Option<Mode> {
        match self {
            Self::Unix { mode, .. }
            | Self::Vsock { mode, .. }
            | Self::Net { mode, .. } => Some(*mode),
            Self::File(_) | Self::Exec(_) | Self::Stdio => None,
        }
    }
}

fn suffix(mode: Mode) -> &'static str {
    match mode {
        Mode::Connect => "CONNECT",
        Mode::Listen => "LISTEN",
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(p) => write!(f, "FILE:{}", quote(p)),
            Self::Unix { mode, path } => {
                write!(f, "UNIX-{}:{}", suffix(*mode), quote(path))
            }
            Self::Vsock {
                mode: Mode::Connect,
                cid,
                port,
            } => {
                write!(f, "VSOCK-CONNECT:{}:{port}", cid.unwrap_or_default())
            }
            Self::Vsock {
                mode: Mode::Listen,
                port,
                ..
            } => write!(f, "VSOCK-LISTEN:{port}"),
            Self::Net { net, mode, socket } => {
                let bare = NETS.iter().any(|(_, n, b)| n == net && *b);
                match mode {
                    Mode::Connect if bare => {
                        write!(f, "{}:{socket}", net.name())
                    }
                    _ => write!(f, "{}-{}:{socket}", net.name(), suffix(*mode)),
                }
            }
            Self::Exec(c) => write!(f, "EXEC:{}", quote(c)),
            Self::Stdio => write!(f, "STDIO"),
        }
    }
}

impl Address {
    pub fn parse(s: &str) -> Result<Self, Error> {
        if s.is_empty() {
            return Err(Error::new(0, Kind::Empty));
        }
        let Lexed { params, options } = Lexed::lex(s)?;
        let (keyword, params) =
            params.split_first().expect("lex yields a token");
        let path = keyword.text.contains('/');
        let target = Target::from(keyword, params, path)?;
        let mode = target.mode();
        let options = options
            .iter()
            .map(|t| Opt::from(t, mode))
            .collect::<Result<_, _>>()?;
        Ok(Self { target, options })
    }

    pub fn mode(&self) -> Option<Mode> {
        self.target.mode()
    }

    pub fn mux(&self) -> bool {
        self.options
            .iter()
            .any(|o| o.name == "mux" || o.name == "fork")
    }

    pub fn cap(&self) -> Option<u32> {
        self.options
            .iter()
            .rev()
            .find(|o| o.name == "max-children")
            .and_then(|o| o.value.as_deref()?.parse().ok())
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
        write!(f, "{}", self.target)?;
        for o in &self.options {
            write!(f, ",{o}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test;
