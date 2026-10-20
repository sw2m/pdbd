mod support;

use socat::{Exclusion, Kind, Part};
use support::fixture::{same, table};

// #57
#[test]
fn sockets() {
    same(&[
        ("sctp:h:1", "SCTP-CONNECT:h:1"),
        ("sctp-l:80", "SCTP-LISTEN:80"),
        ("SCTP-LISTEN:0.0.0.0:80", "SCTP-LISTEN:0.0.0.0:80"),
        ("dccp:h:1", "DCCP-CONNECT:h:1"),
        ("DCCP-L:80", "DCCP-LISTEN:80"),
        ("udplite:h:1", "UDPLITE-CONNECT:h:1"),
        ("udplite-listen:[::]:9", "UDPLITE-LISTEN:[::]:9"),
        ("c-sctp:h:1", "SCTP-CONNECT:h:1"),
    ]);
}

// #57
#[test]
fn datagrams() {
    same(&[
        ("udp-sendto:h:9", "UDP-SENDTO:h:9"),
        ("UDP-DATAGRAM:10.0.0.255:9", "UDP-DATAGRAM:10.0.0.255:9"),
        ("udp-recvfrom:9", "UDP-RECVFROM:9"),
        ("udp-recv:9", "UDP-RECV:9"),
        ("udplite-sendto:h:9", "UDPLITE-SENDTO:h:9"),
        ("udplite-recv:9", "UDPLITE-RECV:9"),
        ("ip-sendto:h:47", "IP-SENDTO:h:47"),
        ("ip-datagram:10.0.0.255:47", "IP-DATAGRAM:10.0.0.255:47"),
        ("ip-recvfrom:47", "IP-RECVFROM:47"),
        ("ip-recv:47", "IP-RECV:47"),
        ("unix-sendto:/s", "UNIX-SENDTO:/s"),
        ("unix-recvfrom:/s", "UNIX-RECVFROM:/s"),
        ("unix-recv:/s", "UNIX-RECV:/s"),
        ("unix-client:/s", "UNIX-CLIENT:/s"),
        ("abstract-sendto:n", "ABSTRACT-SENDTO:n"),
        ("abstract-recvfrom:n", "ABSTRACT-RECVFROM:n"),
        ("abstract-recv:n", "ABSTRACT-RECV:n"),
        ("abstract-client:n", "ABSTRACT-CLIENT:n"),
    ]);
}

// #57
#[test]
fn generic() {
    same(&[
        ("socket-connect:1:0:2f746d70", "SOCKET-CONNECT:1:0:2f746d70"),
        ("SOCKET-LISTEN:1:0:2F746D70", "SOCKET-LISTEN:1:0:2f746d70"),
        ("socket-sendto:2:2:17:0035", "SOCKET-SENDTO:2:2:17:0035"),
        ("socket-recvfrom:2:2:17:0035", "SOCKET-RECVFROM:2:2:17:0035"),
        ("socket-recv:2:2:17:0035", "SOCKET-RECV:2:2:17:0035"),
        ("socket-datagram:2:2:17:0035", "SOCKET-DATAGRAM:2:2:17:0035"),
    ]);
    table(&[
        ("SOCKET-CONNECT:1:0:2f7", 19, Kind::Hex("2f7".into())),
        ("SOCKET-CONNECT:1:0:xy", 19, Kind::Hex("xy".into())),
        ("SOCKET-CONNECT:1:0", 18, Kind::Missing(Part::Address)),
        ("SOCKET-CONNECT:010:0:2f", 15, Kind::Base("010".into())),
        ("SOCKET-CONNECT:1:2:33:2f", 22, Kind::Extra("2f".into())),
        ("SOCKET-SENDTO:2:2:17", 20, Kind::Missing(Part::Address)),
    ]);
}

// #57
#[test]
fn proxies() {
    same(&[
        ("socks4:p:h:80", "SOCKS4:p:h:80"),
        ("socks4a:p:h:80", "SOCKS4A:p:h:80"),
        ("socks5-connect:p:h:80", "SOCKS5-CONNECT:p:h:80"),
        ("socks5-connect:p:1080:h:80", "SOCKS5-CONNECT:p:1080:h:80"),
        ("socks5-listen:p:h:80", "SOCKS5-LISTEN:p:h:80"),
        ("socks5-listen:p:9050:h:80", "SOCKS5-LISTEN:p:9050:h:80"),
        ("proxy:p:h:443", "PROXY:p:h:443"),
    ]);
    table(&[
        ("SOCKS5-CONNECT:p:h", 18, Kind::Missing(Part::Port)),
        ("SOCKS4:p:a_b:80", 9, Kind::Host("a_b".into())),
        ("PROXY:p:h", 9, Kind::Missing(Part::Port)),
    ]);
}

// #57
#[test]
fn descriptors() {
    same(&[
        ("fd:3", "FD:3"),
        ("3", "FD:3"),
        ("3,nonblock", "FD:3,nonblock"),
        ("accept:0", "ACCEPT-FD:0"),
        ("accept-fd:0,fork", "ACCEPT-FD:0,mux"),
        ("3/x", "GOPEN:3/x"),
    ]);
    table(&[
        ("010", 0, Kind::Base("010".into())),
        ("FD:010", 3, Kind::Base("010".into())),
        ("3foo", 0, Kind::Keyword("3foo".into())),
        ("FD:", 3, Kind::Blank),
    ]);
}

// #57
#[test]
fn processes() {
    same(&[
        ("system:ls", "SYSTEM:ls"),
        ("SYSTEM:'ls -l'", "SYSTEM:'ls -l'"),
        ("shell", "SHELL"),
        ("shell:ls", "SHELL:ls"),
    ]);
    table(&[("SHELL:", 6, Kind::Blank), ("EXEC:''", 5, Kind::Blank)]);
}

// #57
#[test]
fn streams() {
    same(&[
        ("stdin", "STDIN"),
        ("stdout", "STDOUT"),
        ("stderr", "STDERR"),
        ("create:/tmp/x", "CREATE:/tmp/x"),
        ("OPEN:/f,rdonly", "OPEN:/f,rdonly"),
        ("FILE:/f,WRONLY", "FILE:/f,wronly"),
    ]);
    table(&[
        ("STDIN:x", 6, Kind::Extra("x".into())),
        ("OPEN:/f,rdonly,wronly", 15, Kind::Option("wronly".into())),
        ("TCP:h:1,rdonly", 8, Kind::Option("rdonly".into())),
    ]);
}

// #57
#[test]
fn queues() {
    same(&[
        ("pipe", "PIPE"),
        ("pipe:/tmp/p", "PIPE:/tmp/p"),
        ("PIPE:/tmp/p,rdonly", "PIPE:/tmp/p,rdonly"),
        ("socketpair", "SOCKETPAIR"),
        ("posixmq-read:/q", "POSIXMQ-READ:/q"),
        ("posixmq-receive:/q", "POSIXMQ-RECEIVE:/q"),
        ("posixmq-recv:/q", "POSIXMQ-RECEIVE:/q"),
        ("posixmq-send:/q", "POSIXMQ-SEND:/q"),
        ("posixmq-write:/q", "POSIXMQ-WRITE:/q"),
        ("posixmq-bidirectional:/q", "POSIXMQ-BIDIRECTIONAL:/q"),
        ("posixmq:/q", "POSIXMQ:/q"),
    ]);
}

// #57
#[test]
fn others() {
    same(&[
        ("tun", "TUN"),
        ("tun:10.0.0.1/24", "TUN:10.0.0.1/24"),
        ("readline", "READLINE"),
        ("stall", "STALL"),
        ("text:hi", "TEXT:hi"),
    ]);
    table(&[
        ("TUN:10.0.0.1/33", 13, Kind::Bits("33".into())),
        ("TUN:10.0.0.1", 12, Kind::Missing(Part::Bits)),
        ("TUN:fe80::1/64", 4, Kind::Host("fe80".into())),
        ("TEXT:", 5, Kind::Blank),
    ]);
}

// #57
#[test]
fn forking() {
    same(&[
        ("UDP-RECVFROM:9,fork", "UDP-RECVFROM:9,mux"),
        ("UNIX-RECVFROM:/s,fork", "UNIX-RECVFROM:/s,mux"),
        (
            "POSIXMQ-RECEIVE:/q,fork,max-children=2",
            "POSIXMQ-RECEIVE:/q,mux,max-children=2",
        ),
        ("POSIXMQ-SEND:/q,fork", "POSIXMQ-SEND:/q,mux"),
        ("SCTP-LISTEN:80,fork", "SCTP-LISTEN:80,mux"),
    ]);
    table(&[
        ("UDP-SENDTO:h:9,fork", 15, Kind::Option("fork".into())),
        ("POSIXMQ-READ:/q,fork", 16, Kind::Option("fork".into())),
        ("STDIN,fork", 6, Kind::Option("fork".into())),
    ]);
}

// #57
#[test]
fn excluded() {
    let openssl = Kind::Excluded(Exclusion::Openssl);
    let family = |k: &str| Kind::Excluded(Exclusion::Family(k.into()));
    table(&[
        ("OPENSSL:h:443", 0, openssl.clone()),
        ("openssl-listen:443", 0, openssl.clone()),
        ("OPENSSL-DTLS-CLIENT:h:1", 0, openssl.clone()),
        ("OPENSSL-DTLS-SERVER:1", 0, openssl.clone()),
        ("TCP4:h:1", 0, family("TCP-CONNECT")),
        ("tcp6-listen:443", 0, family("TCP-LISTEN")),
        ("UDP4-SENDTO:h:1", 0, family("UDP-SENDTO")),
        ("udp6-recv:1", 0, family("UDP-RECV")),
        ("UDPLITE4:h:1", 0, family("UDPLITE-CONNECT")),
        ("SCTP6-LISTEN:1", 0, family("SCTP-LISTEN")),
        ("DCCP4-L:1", 0, family("DCCP-LISTEN")),
        ("IP4-RECV:47", 0, family("IP-RECV")),
        ("IP6-DATAGRAM:h:47", 0, family("IP-DATAGRAM")),
        ("INTERFACE:eth0", 0, Kind::Deferred(80)),
        ("C-STDIO", 0, Kind::Keyword("C-STDIO".into())),
        ("http://h", 0, Kind::Keyword("http".into())),
    ]);
}
