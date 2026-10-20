mod support;

use socat::{Channel, Error, Half, Kind, Reason, Unlinked};

fn parse(s: &str) -> Channel {
    Channel::parse(s).unwrap_or_else(|e| panic!("{s:?}: {e}"))
}

fn fail(s: &str) -> Error {
    match Channel::parse(s) {
        Ok(c) => panic!("{s:?} parsed as {c:?}"),
        Err(e) => e,
    }
}

// #57
#[test]
fn duals() {
    for (s, canonical) in [
        ("stdin!!stdout", "STDIN!!STDOUT"),
        ("PIPE:/tmp/a!!PIPE:/tmp/b", "PIPE:/tmp/a!!PIPE:/tmp/b"),
        ("/tmp/a!!/tmp/b", "GOPEN:/tmp/a!!GOPEN:/tmp/b"),
        ("0!!1", "FD:0!!FD:1"),
        ("udp-recv:9!!udp-sendto:h:9", "UDP-RECV:9!!UDP-SENDTO:h:9"),
        ("EXEC:'a !! b'!!STDOUT", "EXEC:'a !! b'!!STDOUT"),
        ("tcp:h:1", "TCP-CONNECT:h:1"),
    ] {
        assert_eq!(parse(s).to_string(), canonical, "{s}");
        assert_eq!(parse(s), parse(canonical), "{s}");
    }
    assert_eq!(fail("STDIN!!STDOUT!!STDERR"), Error::new(13, Kind::Dual));
    assert_eq!(fail("STDIN!!"), Error::new(7, Kind::Empty));
    assert_eq!(fail("!!STDOUT"), Error::new(0, Kind::Empty));
}

// #57
#[test]
fn links() {
    for s in [
        "TCP:h:1",
        "TCP-LISTEN:80,mux",
        "UDP-SENDTO:h:9",
        "UDP-RECVFROM:9",
        "UNIX-CLIENT:/s",
        "SOCKS5-CONNECT:p:h:80",
        "EXEC:cat",
        "SYSTEM:cat",
        "STDIO",
        "PTY",
        "TUN:10.0.0.1/24",
        "ACCEPT-FD:0",
        "FD:1",
        "3",
        "OPEN:/dev/ttyS0",
        "STDIN!!STDOUT",
        "PIPE:/tmp/a!!PIPE:/tmp/b",
        "POSIXMQ-READ:/a!!POSIXMQ-SEND:/b",
        "OPEN:/f,rdonly!!OPEN:/g,wronly",
        "UDP-RECV:9!!UDP-SENDTO:h:9",
        "TCP:h:1!!TCP:h:2",
    ] {
        assert_eq!(parse(s).link(), Ok(()), "{s}");
    }
}

// #57
#[test]
fn unlinked() {
    let single = |reason| Unlinked::new(None, reason);
    let read = |reason| Unlinked::new(Some(Half::Read), reason);
    let write = |reason| Unlinked::new(Some(Half::Write), reason);
    for (s, want) in [
        ("STDIN", single(Reason::Oneway)),
        ("STDOUT", single(Reason::Oneway)),
        ("CREATE:/x", single(Reason::Oneway)),
        ("UDP-RECV:9", single(Reason::Oneway)),
        ("POSIXMQ-SEND:/q", single(Reason::Oneway)),
        ("OPEN:/f,wronly", single(Reason::Oneway)),
        ("PIPE:/tmp/p", single(Reason::Queue)),
        ("POSIXMQ:/q", single(Reason::Queue)),
        ("PIPE", single(Reason::Echo)),
        ("SOCKETPAIR", single(Reason::Echo)),
        ("POSIXMQ-BIDIRECTIONAL:/q", single(Reason::Echo)),
        ("STALL", single(Reason::Inert)),
        ("READLINE", single(Reason::Interactive)),
        ("TEXT:hi", single(Reason::Constant)),
        ("STDOUT!!STDIN", read(Reason::Oneway)),
        ("STDIN!!STDIN", write(Reason::Oneway)),
        ("TCP:h:1!!PIPE", write(Reason::Echo)),
        ("TEXT:x!!STDOUT", read(Reason::Constant)),
        ("READLINE!!STDOUT", read(Reason::Interactive)),
        ("PIPE!!SOCKETPAIR", read(Reason::Echo)),
    ] {
        assert_eq!(parse(s).link(), Err(want), "{s}");
    }
    assert!(Reason::Oneway.pairs());
    assert!(Reason::Queue.pairs());
    for r in [
        Reason::Echo,
        Reason::Inert,
        Reason::Interactive,
        Reason::Constant,
    ] {
        assert!(!r.pairs(), "{r:?}");
    }
}
