use super::*;

pub(crate) fn check(s: &str) -> Result<(), TestCaseError> {
    match Address::parse(s) {
        Ok(a) => {
            let p = a.to_string();
            prop_assert_eq!(Address::parse(&p), Ok(a), "{:?} → {:?}", s, p);
        }
        Err(e) => {
            prop_assert!(e.at <= s.len(), "{:?}: {}", s, e);
            prop_assert!(s.is_char_boundary(e.at), "{:?}: {}", s, e);
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) enum Edit {
    Insert(usize, u8),
    Delete(usize),
    Substitute(usize, u8),
}

impl Edit {
    pub(crate) fn apply(&self, s: &str) -> Vec<u8> {
        let mut b = s.as_bytes().to_vec();
        match *self {
            Self::Insert(i, x) => b.insert(i % (b.len() + 1), x),
            Self::Delete(i) if !b.is_empty() => {
                let i = i % b.len();
                b.remove(i);
            }
            Self::Substitute(i, x) if !b.is_empty() => {
                let i = i % b.len();
                b[i] = x;
            }
            _ => {}
        }
        b
    }
}

pub(crate) fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        (any::<usize>(), any::<u8>()).prop_map(|(i, b)| Edit::Insert(i, b)),
        any::<usize>().prop_map(Edit::Delete),
        (any::<usize>(), any::<u8>()).prop_map(|(i, b)| Edit::Substitute(i, b)),
    ]
}

// socat 1.8.1.3's `-hhh` address heads and their aliases, the grammar's authority (#57 v2.1)
const SOCAT: &[&str] = &[
    "ABSTRACT",
    "ABSTRACT-CLIENT",
    "ABSTRACT-CONNECT",
    "ABSTRACT-LISTEN",
    "ABSTRACT-RECV",
    "ABSTRACT-RECVFROM",
    "ABSTRACT-SENDTO",
    "ACCEPT",
    "ACCEPT-FD",
    "CREAT",
    "CREATE",
    "DATAGRAM",
    "DCCP",
    "DCCP-CONNECT",
    "DCCP-L",
    "DCCP-LISTEN",
    "DCCP4",
    "DCCP4-CONNECT",
    "DCCP4-L",
    "DCCP4-LISTEN",
    "DCCP6",
    "DCCP6-CONNECT",
    "DCCP6-L",
    "DCCP6-LISTEN",
    "DGRAM",
    "DTLS",
    "DTLS-C",
    "DTLS-CLIENT",
    "DTLS-CONNECT",
    "DTLS-L",
    "DTLS-LISTEN",
    "DTLS-SERVER",
    "ECHO",
    "EXEC",
    "FD",
    "FIFO",
    "FILE",
    "GOPEN",
    "IF",
    "INET",
    "INET-L",
    "INET-LISTEN",
    "INET4",
    "INET4-L",
    "INET4-LISTEN",
    "INET6",
    "INET6-L",
    "INET6-LISTEN",
    "INTERFACE",
    "IP",
    "IP-DATAGRAM",
    "IP-DGRAM",
    "IP-RECV",
    "IP-RECVFROM",
    "IP-SEND",
    "IP-SENDTO",
    "IP4",
    "IP4-DATAGRAM",
    "IP4-DGRAM",
    "IP4-RECV",
    "IP4-RECVFROM",
    "IP4-SEND",
    "IP4-SENDTO",
    "IP6",
    "IP6-DATAGRAM",
    "IP6-DGRAM",
    "IP6-RECV",
    "IP6-RECVFROM",
    "IP6-SEND",
    "IP6-SENDTO",
    "LOCAL",
    "OPEN",
    "OPENSSL",
    "OPENSSL-CONNECT",
    "OPENSSL-DTLS-CLIENT",
    "OPENSSL-DTLS-CONNECT",
    "OPENSSL-DTLS-LISTEN",
    "OPENSSL-DTLS-SERVER",
    "OPENSSL-LISTEN",
    "PIPE",
    "POSIXMQ",
    "POSIXMQ-BIDIRECTIONAL",
    "POSIXMQ-READ",
    "POSIXMQ-RECEIVE",
    "POSIXMQ-RECV",
    "POSIXMQ-SEND",
    "POSIXMQ-WRITE",
    "PROXY",
    "PROXY-CONNECT",
    "PTY",
    "READLINE",
    "SCTP",
    "SCTP-CONNECT",
    "SCTP-L",
    "SCTP-LISTEN",
    "SCTP4",
    "SCTP4-CONNECT",
    "SCTP4-L",
    "SCTP4-LISTEN",
    "SCTP6",
    "SCTP6-CONNECT",
    "SCTP6-L",
    "SCTP6-LISTEN",
    "SENDTO",
    "SHELL",
    "SOCKET-CONNECT",
    "SOCKET-DATAGRAM",
    "SOCKET-LISTEN",
    "SOCKET-RECV",
    "SOCKET-RECVFROM",
    "SOCKET-SENDTO",
    "SOCKETPAIR",
    "SOCKS",
    "SOCKS4",
    "SOCKS4A",
    "SOCKS5",
    "SOCKS5-BIND",
    "SOCKS5-CONNECT",
    "SOCKS5-LISTEN",
    "SSL",
    "SSL-L",
    "STALL",
    "STDERR",
    "STDIN",
    "STDIO",
    "STDOUT",
    "SYSTEM",
    "TCP",
    "TCP-CONNECT",
    "TCP-L",
    "TCP-LISTEN",
    "TCP4",
    "TCP4-CONNECT",
    "TCP4-L",
    "TCP4-LISTEN",
    "TCP6",
    "TCP6-CONNECT",
    "TCP6-L",
    "TCP6-LISTEN",
    "TEXT",
    "TUN",
    "UDP",
    "UDP-CONNECT",
    "UDP-DATAGRAM",
    "UDP-DGRAM",
    "UDP-L",
    "UDP-LISTEN",
    "UDP-RECV",
    "UDP-RECVFROM",
    "UDP-SEND",
    "UDP-SENDTO",
    "UDP4",
    "UDP4-CONNECT",
    "UDP4-DATAGRAM",
    "UDP4-DGRAM",
    "UDP4-L",
    "UDP4-LISTEN",
    "UDP4-RECV",
    "UDP4-RECVFROM",
    "UDP4-SEND",
    "UDP4-SENDTO",
    "UDP6",
    "UDP6-CONNECT",
    "UDP6-DATAGRAM",
    "UDP6-DGRAM",
    "UDP6-L",
    "UDP6-LISTEN",
    "UDP6-RECV",
    "UDP6-RECVFROM",
    "UDP6-SEND",
    "UDP6-SENDTO",
    "UDPLITE",
    "UDPLITE-CONNECT",
    "UDPLITE-DATAGRAM",
    "UDPLITE-DGRAM",
    "UDPLITE-L",
    "UDPLITE-LISTEN",
    "UDPLITE-RECV",
    "UDPLITE-RECVFROM",
    "UDPLITE-SEND",
    "UDPLITE-SENDTO",
    "UDPLITE4",
    "UDPLITE4-CONNECT",
    "UDPLITE4-DATAGRAM",
    "UDPLITE4-DGRAM",
    "UDPLITE4-L",
    "UDPLITE4-LISTEN",
    "UDPLITE4-RECV",
    "UDPLITE4-RECVFROM",
    "UDPLITE4-SEND",
    "UDPLITE4-SENDTO",
    "UDPLITE6",
    "UDPLITE6-CONNECT",
    "UDPLITE6-DATAGRAM",
    "UDPLITE6-DGRAM",
    "UDPLITE6-L",
    "UDPLITE6-LISTEN",
    "UDPLITE6-RECV",
    "UDPLITE6-RECVFROM",
    "UDPLITE6-SEND",
    "UDPLITE6-SENDTO",
    "UNIX",
    "UNIX-CLIENT",
    "UNIX-CONNECT",
    "UNIX-L",
    "UNIX-LISTEN",
    "UNIX-RECV",
    "UNIX-RECVFROM",
    "UNIX-SEND",
    "UNIX-SENDTO",
    "VSOCK",
    "VSOCK-CONNECT",
    "VSOCK-L",
    "VSOCK-LISTEN",
];

pub(crate) fn keywords() -> Vec<String> {
    let mut all: Vec<String> = SOCAT.iter().map(|k| k.to_string()).collect();
    all.extend(["HTTPS", "SSL", "SSL-L"].map(String::from));
    let roots = [
        "TCP",
        "UDP",
        "UDPLITE",
        "SCTP",
        "DCCP",
        "UNIX",
        "ABSTRACT",
        "VSOCK",
        "WS",
        "WSS",
        "WEBTRANSPORT",
    ];
    let pinned = ["TCP", "UDP", "UDPLITE", "SCTP", "DCCP"];
    for root in roots {
        all.extend(modal(root, Mode::Connect, &[]));
        all.extend(modal(root, Mode::Listen, &[]));
    }
    for root in pinned {
        for family in ["4", "6"] {
            let root = format!("{root}{family}");
            all.extend(modal(&root, Mode::Connect, &[]));
            all.extend(modal(&root, Mode::Listen, &[]));
        }
    }
    all
}

pub(crate) fn excluded(k: &str) -> bool {
    k.starts_with("OPENSSL") || k.starts_with("DTLS")
}
