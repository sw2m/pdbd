mod support;

use socat::{Exclusion, Kind};
use support::fixture::{same, table};

// #57
#[test]
fn aliases() {
    same(&[
        ("FILE:/x", "OPEN:/x"),
        ("file:/dev/ttyS0,b115200,raw", "OPEN:/dev/ttyS0,b115200,raw"),
        ("UNIX:/s", "UNIX-CLIENT:/s"),
        ("ABSTRACT:n", "ABSTRACT-CLIENT:n"),
        ("LOCAL:/s", "UNIX-CONNECT:/s"),
        ("UNIX-L:/s", "UNIX-LISTEN:/s"),
        ("UNIX-SEND:/s", "UNIX-SENDTO:/s"),
        ("INET:h:1", "TCP-CONNECT:h:1"),
        ("INET-L:1", "TCP-LISTEN:1"),
        ("inet-listen:1", "TCP-LISTEN:1"),
        ("IP:h:47", "IP-SENDTO:h:47"),
        ("IP-SEND:h:47", "IP-SENDTO:h:47"),
        ("IP-DGRAM:h:47", "IP-DATAGRAM:h:47"),
        ("UDP-SEND:h:1", "UDP-SENDTO:h:1"),
        ("UDP-DGRAM:h:1", "UDP-DATAGRAM:h:1"),
        ("UDPLITE-SEND:h:1", "UDPLITE-SENDTO:h:1"),
        ("UDPLITE-DGRAM:h:1", "UDPLITE-DATAGRAM:h:1"),
        ("DATAGRAM:2:2:17:x0035", "SOCKET-DATAGRAM:2:2:17:x0035"),
        ("DGRAM:2:2:17:x0035", "SOCKET-DATAGRAM:2:2:17:x0035"),
        ("SENDTO:2:2:17:x0035", "SOCKET-SENDTO:2:2:17:x0035"),
        ("SOCKS:p:h:1", "SOCKS4:p:h:1"),
        ("SOCKS5:p:h:1", "SOCKS5-CONNECT:p:h:1"),
        ("SOCKS5-BIND:p:h:1", "SOCKS5-LISTEN:p:h:1"),
        ("PROXY-CONNECT:p:h:1", "PROXY:p:h:1"),
        ("ECHO", "PIPE"),
        ("FIFO", "PIPE"),
        ("FIFO:/p", "PIPE:/p"),
        ("CREAT:/x", "CREATE:/x"),
        ("VSOCK:3:1", "VSOCK-CONNECT:3:1"),
        ("VSOCK-L:1", "VSOCK-LISTEN:1"),
        ("SCTP-L:1", "SCTP-LISTEN:1"),
    ]);
}

// #57
#[test]
fn left() {
    let openssl = Kind::Excluded(Exclusion::Openssl);
    let family = |k: &str| Kind::Excluded(Exclusion::Family(k.into()));
    table(&[
        ("INET4:h:1", 0, family("TCP-CONNECT")),
        ("INET6-L:1", 0, family("TCP-LISTEN")),
        ("IP4:h:47", 0, family("IP-SENDTO")),
        ("UDP4-SEND:h:1", 0, family("UDP-SENDTO")),
        ("UDPLITE6-DGRAM:h:1", 0, family("UDPLITE-DATAGRAM")),
        ("DTLS:h:1", 0, openssl.clone()),
        ("DTLS-L:1", 0, openssl.clone()),
        ("dtls-server:1", 0, openssl.clone()),
        ("IF:lo", 0, Kind::Deferred(80)),
    ]);
}
