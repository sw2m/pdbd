mod gram;
mod mode;
mod mq;
mod net;
mod root;
mod socks;
mod vsock;
mod web;

pub use gram::{Gram, Point};
pub use mode::Mode;
pub use mq::Mq;
pub use net::{Inet, Net};
pub use socks::Socks;
pub use vsock::Vsock;
pub use web::Web;

use crate::lex::{Params, Quoted, Token};
use crate::{
    Dalan, Error, Exclusion, Host, Kind, Nature, Part, Port, Resource, Text,
};
use root::Root;
use std::fmt;
use std::net::Ipv4Addr;

const DEFERRED: u32 = 80; // INTERFACE, raw Ethernet frames: sw2m/pdbd#80

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Open(Text),
    Gopen(Text),
    Create(Text),
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
    Gram {
        gram: Gram,
        mode: Mode,
        host: Option<Host>,
        point: Point,
    },
    Socket {
        mode: Mode,
        domain: u32,
        kind: Option<u32>,
        protocol: u32,
        address: Dalan,
    },
    Socks {
        socks: Socks,
        mode: Mode,
        server: Host,
        via: Option<Port>,
        host: Host,
        port: Port,
    },
    Proxy {
        proxy: Host,
        host: Host,
        port: Port,
    },
    Web {
        web: Web,
        inet: Inet,
        resource: Resource,
    },
    Exec(Text),
    System(Text),
    Shell(Option<Text>),
    Fd(u32),
    Accept(u32),
    Stdio,
    Stdin,
    Stdout,
    Stderr,
    Pty,
    Pipe(Option<Text>),
    Socketpair,
    Mq {
        mq: Mq,
        name: Text,
    },
    Tun(Option<(Ipv4Addr, u8)>),
    Readline,
    Stall,
    Text(Text),
}

impl Target {
    pub(crate) fn parse(tokens: &[Token]) -> Result<Self, Error> {
        let (keyword, rest) = tokens.split_first().expect("lex yields a token");
        let word = keyword.text.as_str();
        let params = Params::new(rest, keyword.end);
        if !word.is_empty() && word.bytes().all(|b| b.is_ascii_digit()) {
            let fd = keyword.exact(Part::Fd, Kind::Number)?;
            return params.done(0).map(|()| Self::Fd(fd));
        }
        if word.contains('/') {
            return Ok(Self::Gopen(Params::new(tokens, 0).text(Part::Path)?));
        }
        let upper = word.to_ascii_uppercase();
        let url = rest.first().is_some_and(|t| t.text.starts_with("//"));
        let none = |t: Self| params.done(0).map(|()| t);
        Ok(match upper.as_str() {
            "STDIO" | "-" => none(Self::Stdio)?,
            "STDIN" => none(Self::Stdin)?,
            "STDOUT" => none(Self::Stdout)?,
            "STDERR" => none(Self::Stderr)?,
            "PTY" => none(Self::Pty)?,
            "SOCKETPAIR" => none(Self::Socketpair)?,
            "READLINE" => none(Self::Readline)?,
            "STALL" => none(Self::Stall)?,
            "OPEN" | "FILE" => Self::Open(params.text(Part::Path)?),
            "GOPEN" => Self::Gopen(params.text(Part::Path)?),
            "CREATE" | "CREAT" => Self::Create(params.text(Part::Path)?),
            "EXEC" => Self::Exec(params.text(Part::Command)?),
            "SYSTEM" => Self::System(params.text(Part::Command)?),
            "SHELL" => Self::Shell(params.optional(Part::Command)?),
            "PIPE" | "ECHO" | "FIFO" => {
                Self::Pipe(params.optional(Part::Path)?)
            }
            "TEXT" => Self::Text(params.text(Part::Text)?),
            "FD" => Self::Fd(
                params
                    .number(0, Part::Fd)
                    .and_then(|n| params.done(1).map(|()| n))?,
            ),
            "ACCEPT" | "ACCEPT-FD" => Self::Accept(
                params
                    .number(0, Part::Fd)
                    .and_then(|n| params.done(1).map(|()| n))?,
            ),
            "TUN" => Self::tun(&params)?,
            "SOCKS4" | "SOCKS" => {
                Self::socks(Socks::V4, Mode::Connect, &params)?
            }
            "SOCKS4A" => Self::socks(Socks::V4a, Mode::Connect, &params)?,
            "PROXY" | "PROXY-CONNECT" => Self::proxy(&params)?,
            "DATAGRAM" | "DGRAM" => Self::socket(Mode::Datagram, &params)?,
            "SENDTO" => Self::socket(Mode::Sendto, &params)?,
            "INTERFACE" | "IF" => {
                return Err(keyword.fail(Kind::Deferred(DEFERRED)))
            }
            "WS" if url => Self::url(Web::Ws, &params)?,
            "WSS" if url => Self::url(Web::Wss, &params)?,
            "HTTPS" if url => Self::url(Web::Webtransport, &params)?,
            u if u.starts_with("OPENSSL")
                || u.starts_with("DTLS")
                || u == "SSL"
                || u == "SSL-L" =>
            {
                return Err(keyword.fail(Kind::Excluded(Exclusion::Openssl)));
            }
            u => match Mq::word(u) {
                Some(mq) => Self::queue(mq, &params)?,
                None => Self::modal(keyword, u, &params)?,
            },
        })
    }

    fn modal(
        keyword: &Token,
        upper: &str,
        params: &Params,
    ) -> Result<Self, Error> {
        let (root, mode) = match Root::resolve(upper) {
            Some(Ok(found)) => found,
            Some(Err(why)) => return Err(keyword.fail(Kind::Excluded(why))),
            None => {
                return Err(keyword.fail(Kind::Keyword(keyword.text.clone())))
            }
        };
        let net = |net| -> Result<Self, Error> {
            let inet = Inet::parse(params, mode)?;
            Ok(Self::Net { net, inet })
        };
        let stream = matches!(mode, Mode::Connect | Mode::Listen);
        Ok(match root {
            Root::Unix => Self::Unix {
                mode,
                path: params.text(Part::Path)?,
            },
            Root::Abstract => Self::Abstract {
                mode,
                name: params.text(Part::Name)?,
            },
            Root::Vsock => Self::Vsock(Vsock::parse(params, mode)?),
            Root::Tcp => net(Net::Tcp)?,
            Root::Sctp => net(Net::Sctp)?,
            Root::Dccp => net(Net::Dccp)?,
            Root::Udp if stream => net(Net::Udp)?,
            Root::Udplite if stream => net(Net::Udplite)?,
            Root::Udp => Self::gram(Gram::Udp, mode, params)?,
            Root::Udplite => Self::gram(Gram::Udplite, mode, params)?,
            Root::Ip => Self::gram(Gram::Ip, mode, params)?,
            Root::Ws => Self::web(Web::Ws, params, mode)?,
            Root::Wss => Self::web(Web::Wss, params, mode)?,
            Root::Webtransport => Self::web(Web::Webtransport, params, mode)?,
            Root::Socket => Self::socket(mode, params)?,
            Root::Socks5 => Self::socks(Socks::V5, mode, params)?,
        })
    }

    fn gram(gram: Gram, mode: Mode, params: &Params) -> Result<Self, Error> {
        let point = |i: usize| -> Result<Point, Error> {
            match gram {
                Gram::Ip => {
                    params.number(i, Part::Protocol).map(Point::Protocol)
                }
                _ => Port::parse(params.get(i, Part::Port)?).map(Point::Port),
            }
        };
        let (host, point) = match mode {
            Mode::Sendto | Mode::Datagram => {
                let host = Host::parse(params.get(0, Part::Host)?)?;
                (Some(host), point(1)?)
            }
            _ => (None, point(0)?),
        };
        params.done(usize::from(host.is_some()) + 1)?;
        Ok(Self::Gram {
            gram,
            mode,
            host,
            point,
        })
    }

    fn socket(mode: Mode, params: &Params) -> Result<Self, Error> {
        let domain = params.number(0, Part::Domain)?;
        let stream = matches!(mode, Mode::Connect | Mode::Listen);
        let kind = match stream {
            true => None,
            false => Some(params.number(1, Part::Type)?),
        };
        let at = usize::from(!stream) + 1;
        let protocol = params.number(at, Part::Protocol)?;
        let tok = params.get(at + 1, Part::Address)?;
        if tok.text.is_empty() {
            return Err(tok.fail(Kind::Missing(Part::Address)));
        }
        let address = Dalan::parse(tok)?;
        params.done(at + 2)?;
        Ok(Self::Socket {
            mode,
            domain,
            kind,
            protocol,
            address,
        })
    }

    fn socks(socks: Socks, mode: Mode, params: &Params) -> Result<Self, Error> {
        let server = Host::parse(params.get(0, Part::Host)?)?;
        let long = socks == Socks::V5 && params.len() >= 4;
        let via = match long {
            true => Some(Port::parse(params.get(1, Part::Port)?)?),
            false => None,
        };
        let at = usize::from(long) + 1;
        let host = Host::parse(params.get(at, Part::Host)?)?;
        let port = Port::parse(params.get(at + 1, Part::Port)?)?;
        params.done(at + 2)?;
        Ok(Self::Socks {
            socks,
            mode,
            server,
            via,
            host,
            port,
        })
    }

    fn queue(mq: Mq, params: &Params) -> Result<Self, Error> {
        let name = params.text(Part::Name)?;
        let queue = Self::Mq { mq, name };
        queue
            .check()
            .map_err(|k| Error::new(params.tokens()[0].at, k))?;
        Ok(queue)
    }

    fn proxy(params: &Params) -> Result<Self, Error> {
        let proxy = Host::parse(params.get(0, Part::Host)?)?;
        let host = Host::parse(params.get(1, Part::Host)?)?;
        let port = Port::parse(params.get(2, Part::Port)?)?;
        params.done(3)?;
        Ok(Self::Proxy { proxy, host, port })
    }

    // TUN assigns its address with IPv4 calls, so only an IPv4 literal is taken
    fn tun(params: &Params) -> Result<Self, Error> {
        let Some(tok) = params.tokens().first() else {
            return Ok(Self::Tun(None));
        };
        if params.len() > 1 && !tok.text.contains('/') {
            return Err(match Host::parse(tok) {
                Ok(Host::V4(_)) => {
                    Error::new(tok.end, Kind::Missing(Part::Bits))
                }
                _ => tok.fail(Kind::Host(tok.text.clone())),
            });
        }
        params.done(1)?;
        if tok.text.is_empty() {
            return Err(tok.fail(Kind::Blank));
        }
        let Some((addr, bits)) = tok.text.split_once('/') else {
            return Err(Error::new(tok.end, Kind::Missing(Part::Bits)));
        };
        let ip = match Host::parse(&Token::new(addr, tok.at)) {
            Ok(Host::V4(ip)) => ip,
            _ => return Err(tok.fail(Kind::Host(addr.into()))),
        };
        let at = tok.at + addr.len() + 1;
        let n: u8 = Token::new(bits, at).exact(Part::Bits, Kind::Bits)?;
        let tun = Self::Tun(Some((ip, n)));
        tun.check().map_err(|k| Error::new(at, k))?;
        Ok(tun)
    }

    // see mq_overview(7): a queue name is "/somename"
    pub(crate) fn check(&self) -> Result<(), Kind> {
        match self {
            Self::Mq { name, .. } if !name.as_str().starts_with('/') => {
                Err(Kind::Queue(name.as_str().into()))
            }
            Self::Tun(Some((_, bits))) if *bits > 32 => {
                Err(Kind::Bits(bits.to_string()))
            }
            _ => Ok(()),
        }
    }

    pub fn single(&self) -> bool {
        match self {
            Self::Gram { mode, .. }
            | Self::Unix { mode, .. }
            | Self::Abstract { mode, .. }
            | Self::Socket { mode, .. } => *mode == Mode::Recv,
            Self::Open(_)
            | Self::Gopen(_)
            | Self::Create(_)
            | Self::Stdio
            | Self::Stdin
            | Self::Stdout
            | Self::Stderr
            | Self::Pty
            | Self::Fd(_)
            | Self::Pipe(_)
            | Self::Socketpair
            | Self::Mq { .. }
            | Self::Tun(_)
            | Self::Readline
            | Self::Stall
            | Self::Text(_) => true,
            _ => false,
        }
    }

    pub fn mode(&self) -> Option<Mode> {
        match self {
            Self::Unix { mode, .. }
            | Self::Abstract { mode, .. }
            | Self::Gram { mode, .. }
            | Self::Socket { mode, .. }
            | Self::Socks { mode, .. } => Some(*mode),
            Self::Vsock(v) => Some(v.mode()),
            Self::Net { inet, .. } | Self::Web { inet, .. } => {
                Some(inet.mode())
            }
            _ => None,
        }
    }

    pub fn forks(&self) -> bool {
        match self {
            Self::Accept(_) => true,
            Self::Mq { mq, .. } => {
                matches!(mq, Mq::Receive | Mq::Send | Mq::Write)
            }
            _ => matches!(self.mode(), Some(Mode::Listen | Mode::Recvfrom)),
        }
    }

    pub fn access(&self) -> bool {
        matches!(self, Self::Open(_) | Self::Gopen(_) | Self::Pipe(Some(_)))
    }

    pub(crate) fn nature(&self) -> Nature {
        match self {
            Self::Pipe(Some(_)) | Self::Mq { mq: Mq::Plain, .. } => {
                Nature::Queue
            }
            Self::Pipe(None)
            | Self::Socketpair
            | Self::Mq {
                mq: Mq::Bidirectional,
                ..
            } => Nature::Echo,
            Self::Stall => Nature::Inert,
            Self::Readline => Nature::Interactive,
            Self::Text(_) => Nature::Constant,
            _ => Nature::Peer,
        }
    }

    pub(crate) fn reads(&self) -> bool {
        !matches!(
            self,
            Self::Stdout
                | Self::Stderr
                | Self::Create(_)
                | Self::Mq {
                    mq: Mq::Send | Mq::Write,
                    ..
                }
        )
    }

    pub(crate) fn writes(&self) -> bool {
        let receiver = self.mode() == Some(Mode::Recv);
        !receiver
            && !matches!(
                self,
                Self::Stdin
                    | Self::Text(_)
                    | Self::Mq {
                        mq: Mq::Read | Mq::Receive,
                        ..
                    }
            )
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let q = |t: &Text| Quoted(t.as_str()).to_string();
        match self {
            Self::Open(p) => write!(f, "OPEN:{}", q(p)),
            Self::Gopen(p) => write!(f, "GOPEN:{}", q(p)),
            Self::Create(p) => write!(f, "CREATE:{}", q(p)),
            Self::Unix { mode, path } => write!(f, "UNIX-{mode}:{}", q(path)),
            Self::Abstract { mode, name } => {
                write!(f, "ABSTRACT-{mode}:{}", q(name))
            }
            Self::Vsock(v) => write!(f, "{v}"),
            Self::Net { net, inet } => {
                write!(f, "{net}-{}:{inet}", inet.mode())
            }
            Self::Gram {
                gram,
                mode,
                host,
                point,
            } => match host {
                Some(h) => write!(f, "{gram}-{mode}:{h}:{point}"),
                None => write!(f, "{gram}-{mode}:{point}"),
            },
            Self::Socket {
                mode,
                domain,
                kind,
                protocol,
                address,
            } => {
                write!(f, "SOCKET-{mode}:{domain}:")?;
                if let Some(k) = kind {
                    write!(f, "{k}:")?;
                }
                write!(f, "{protocol}:{}", Quoted(&address.to_string()))
            }
            Self::Socks {
                socks,
                mode,
                server,
                via,
                host,
                port,
            } => {
                match socks {
                    Socks::V5 => write!(f, "{socks}-{mode}:{server}:")?,
                    _ => write!(f, "{socks}:{server}:")?,
                }
                if let Some(v) = via {
                    write!(f, "{v}:")?;
                }
                write!(f, "{host}:{port}")
            }
            Self::Proxy { proxy, host, port } => {
                write!(f, "PROXY:{proxy}:{host}:{port}")
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
            Self::Exec(c) => write!(f, "EXEC:{}", q(c)),
            Self::System(c) => write!(f, "SYSTEM:{}", q(c)),
            Self::Shell(None) => f.write_str("SHELL"),
            Self::Shell(Some(c)) => write!(f, "SHELL:{}", q(c)),
            Self::Fd(n) => write!(f, "FD:{n}"),
            Self::Accept(n) => write!(f, "ACCEPT-FD:{n}"),
            Self::Stdio => f.write_str("STDIO"),
            Self::Stdin => f.write_str("STDIN"),
            Self::Stdout => f.write_str("STDOUT"),
            Self::Stderr => f.write_str("STDERR"),
            Self::Pty => f.write_str("PTY"),
            Self::Pipe(None) => f.write_str("PIPE"),
            Self::Pipe(Some(p)) => write!(f, "PIPE:{}", q(p)),
            Self::Socketpair => f.write_str("SOCKETPAIR"),
            Self::Mq { mq, name } => write!(f, "{mq}:{}", q(name)),
            Self::Tun(None) => f.write_str("TUN"),
            Self::Tun(Some((ip, bits))) => write!(f, "TUN:{ip}/{bits}"),
            Self::Readline => f.write_str("READLINE"),
            Self::Stall => f.write_str("STALL"),
            Self::Text(t) => write!(f, "TEXT:{}", q(t)),
        }
    }
}
