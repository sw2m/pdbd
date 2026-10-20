use crate::{Exclusion, Mode};
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

impl Root {
    fn word(word: &str) -> Option<Self> {
        Some(match word {
            "TCP" => Self::Tcp,
            "UDP" => Self::Udp,
            "UDPLITE" => Self::Udplite,
            "SCTP" => Self::Sctp,
            "DCCP" => Self::Dccp,
            "IP" => Self::Ip,
            "UNIX" | "LOCAL" => Self::Unix,
            "ABSTRACT" => Self::Abstract,
            "VSOCK" => Self::Vsock,
            "WS" => Self::Ws,
            "WSS" => Self::Wss,
            "WEBTRANSPORT" => Self::Webtransport,
            "SOCKET" => Self::Socket,
            "SOCKS5" => Self::Socks5,
            _ => return None,
        })
    }

    fn pin(word: &str) -> Option<Self> {
        let base = word.strip_suffix(['4', '6'])?;
        let root = Self::word(base)?;
        matches!(
            root,
            Self::Tcp
                | Self::Udp
                | Self::Udplite
                | Self::Sctp
                | Self::Dccp
                | Self::Ip
        )
        .then_some(root)
    }

    fn modes(self) -> &'static [Mode] {
        match self {
            Self::Udp | Self::Udplite | Self::Socket => GRAM,
            Self::Ip => RAW,
            Self::Unix | Self::Abstract => LOCAL,
            _ => STREAM,
        }
    }

    fn bare(self) -> bool {
        matches!(
            self,
            Self::Tcp
                | Self::Udp
                | Self::Udplite
                | Self::Sctp
                | Self::Dccp
                | Self::Unix
        )
    }

    fn fits(word: &str, mode: Mode) -> Option<Result<(Self, Mode), Exclusion>> {
        if let Some(root) = Self::word(word) {
            return root.modes().contains(&mode).then_some(Ok((root, mode)));
        }
        let root = Self::pin(word)?;
        root.modes()
            .contains(&mode)
            .then(|| Err(Exclusion::Family(format!("{root}-{mode}"))))
    }

    pub(crate) fn resolve(
        upper: &str,
    ) -> Option<Result<(Self, Mode), Exclusion>> {
        if let Some(found) = Self::fits(upper, Mode::Connect) {
            let bare = match &found {
                Ok((root, _)) => root.bare(),
                Err(_) => Self::pin(upper).is_some_and(Self::bare),
            };
            if bare {
                return Some(found);
            }
        }
        let (a, b) = upper.split_once('-')?;
        if let Some(mode) = Mode::suffix(b) {
            if let Some(found) = Self::fits(a, mode) {
                return Some(found);
            }
        }
        Self::fits(b, Mode::prefix(a)?)
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
