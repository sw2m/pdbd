use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Connect,
    Listen,
    Sendto,
    Recvfrom,
    Recv,
    Datagram,
    Client,
}

impl Mode {
    pub(crate) fn suffix(word: &str) -> Option<Self> {
        match word {
            "SENDTO" => Some(Self::Sendto),
            "RECVFROM" => Some(Self::Recvfrom),
            "RECV" => Some(Self::Recv),
            "DATAGRAM" => Some(Self::Datagram),
            "CLIENT" => Some(Self::Client),
            _ => Self::prefix(word),
        }
    }

    pub(crate) fn prefix(word: &str) -> Option<Self> {
        match word {
            "CONNECT" | "C" => Some(Self::Connect),
            "LISTEN" | "L" => Some(Self::Listen),
            _ => None,
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Connect => "CONNECT",
            Self::Listen => "LISTEN",
            Self::Sendto => "SENDTO",
            Self::Recvfrom => "RECVFROM",
            Self::Recv => "RECV",
            Self::Datagram => "DATAGRAM",
            Self::Client => "CLIENT",
        })
    }
}
