use crate::{Exclusion, Gram, Mode};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Root {
    Tcp,
    Udp,
    Udplite,
    Sctp,
    Dccp,
    Ip,
    Unix,
    Abstract,
    Vsock,
    Ws,
    Wss,
    Webtransport,
    Socket,
    Socks5,
}

const STREAM: &[Mode] = &[Mode::Connect, Mode::Listen];
const GRAM: &[Mode] = &[
    Mode::Connect,
    Mode::Listen,
    Mode::Sendto,
    Mode::Datagram,
    Mode::Recvfrom,
    Mode::Recv,
];
const RAW: &[Mode] =
    &[Mode::Sendto, Mode::Datagram, Mode::Recvfrom, Mode::Recv];
const LOCAL: &[Mode] = &[
    Mode::Connect,
    Mode::Listen,
    Mode::Sendto,
    Mode::Recvfrom,
    Mode::Recv,
    Mode::Client,
];

impl From<Gram> for Root {
    fn from(gram: Gram) -> Self {
        match gram {
            Gram::Udp => Self::Udp,
            Gram::Udplite => Self::Udplite,
            Gram::Ip => Self::Ip,
        }
    }
}

impl Root {
    fn word(word: &str) -> Option<(Self, Option<Mode>)> {
        Some(match word {
            "TCP" | "INET" => (Self::Tcp, Some(Mode::Connect)),
            "UDP" => (Self::Udp, Some(Mode::Connect)),
            "UDPLITE" => (Self::Udplite, Some(Mode::Connect)),
            "SCTP" => (Self::Sctp, Some(Mode::Connect)),
            "DCCP" => (Self::Dccp, Some(Mode::Connect)),
            "IP" => (Self::Ip, Some(Mode::Sendto)),
            "UNIX" => (Self::Unix, Some(Mode::Client)),
            "LOCAL" => (Self::Unix, Some(Mode::Connect)),
            "ABSTRACT" => (Self::Abstract, Some(Mode::Client)),
            "VSOCK" => (Self::Vsock, Some(Mode::Connect)),
            "WS" => (Self::Ws, None),
            "WSS" => (Self::Wss, None),
            "WEBTRANSPORT" => (Self::Webtransport, None),
            "SOCKET" => (Self::Socket, None),
            "SOCKS5" => (Self::Socks5, Some(Mode::Connect)),
            _ => return None,
        })
    }

    fn lookup(word: &str) -> Option<(Self, Option<Mode>, bool)> {
        if let Some((root, bare)) = Self::word(word) {
            return Some((root, bare, false));
        }
        let (root, bare) = Self::word(word.strip_suffix(['4', '6'])?)?;
        let pinnable = matches!(
            root,
            Self::Tcp
                | Self::Udp
                | Self::Udplite
                | Self::Sctp
                | Self::Dccp
                | Self::Ip
        );
        pinnable.then_some((root, bare, true))
    }

    fn short(self) -> bool {
        !matches!(self, Self::Ip | Self::Socket | Self::Socks5)
    }

    // the mode words socat's -hhh allows after an alias root word, None for a genuine root
    fn limited(word: &str) -> Option<&'static [&'static str]> {
        let base = word.strip_suffix(['4', '6']).unwrap_or(word);
        match base {
            "INET" => Some(&["L", "LISTEN"]),
            "LOCAL" => Some(&[]),
            _ => None,
        }
    }

    pub(crate) fn modes(self) -> &'static [Mode] {
        match self {
            Self::Udp | Self::Udplite | Self::Socket => GRAM,
            Self::Ip => RAW,
            Self::Unix | Self::Abstract => LOCAL,
            _ => STREAM,
        }
    }

    fn alias(self, word: &str) -> Option<Mode> {
        match (self, word) {
            (Self::Udp | Self::Udplite | Self::Ip | Self::Unix, "SEND") => {
                Some(Mode::Sendto)
            }
            (Self::Udp | Self::Udplite | Self::Ip, "DGRAM") => {
                Some(Mode::Datagram)
            }
            (Self::Socks5, "BIND") => Some(Mode::Listen),
            _ => None,
        }
    }

    fn check(
        self,
        mode: Mode,
        pinned: bool,
    ) -> Option<Result<(Self, Mode), Exclusion>> {
        if !self.modes().contains(&mode) {
            return None;
        }
        Some(match pinned {
            true => Err(Exclusion::Family(format!("{self}-{mode}"))),
            false => Ok((self, mode)),
        })
    }

    pub(crate) fn resolve(
        upper: &str,
    ) -> Option<Result<(Self, Mode), Exclusion>> {
        if let Some((root, bare, pinned)) = Self::lookup(upper) {
            return root.check(bare?, pinned);
        }
        let (a, b) = upper.split_once('-')?;
        if let Some((root, _, pinned)) = Self::lookup(a) {
            let allowed = match Self::limited(a) {
                Some(words) => words.contains(&b),
                None => root.short() || !matches!(b, "C" | "L"),
            };
            let mode = Mode::suffix(b).or_else(|| root.alias(b));
            if let Some(mode) = mode.filter(|_| allowed) {
                return root.check(mode, pinned);
            }
        }
        let mode = Mode::prefix(a)?;
        let (root, _, pinned) = Self::lookup(b)?;
        (root.short() && Self::limited(b).is_none()).then_some(())?;
        root.check(mode, pinned)
    }
}

impl fmt::Display for Root {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Tcp => "TCP",
            Self::Udp => "UDP",
            Self::Udplite => "UDPLITE",
            Self::Sctp => "SCTP",
            Self::Dccp => "DCCP",
            Self::Ip => "IP",
            Self::Unix => "UNIX",
            Self::Abstract => "ABSTRACT",
            Self::Vsock => "VSOCK",
            Self::Ws => "WS",
            Self::Wss => "WSS",
            Self::Webtransport => "WEBTRANSPORT",
            Self::Socket => "SOCKET",
            Self::Socks5 => "SOCKS5",
        })
    }
}
