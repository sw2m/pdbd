use crate::fixture::{exec, num, parse, same, table, text};
use crate::strategy::keywords;
use crate::{Address, Error, Inet, Kind, Mode, Net, Part, Target, Vsock};
use proptest::prelude::*;
use std::num::NonZeroU32;

// #57
#[test]
fn synonyms_and_case() {
    same(&[
        ("-", "STDIO"),
        ("stdio", "STDIO"),
        ("pty", "PTY"),
        ("tcp-l:80", "TCP-LISTEN:80"),
        ("TCP:h:1", "TCP-CONNECT:h:1"),
        ("tcp-c:h:1", "TCP-CONNECT:h:1"),
        ("UDP:h:1", "UDP-CONNECT:h:1"),
        ("udp-l:53", "UDP-LISTEN:53"),
        ("unix-l:/s", "UNIX-LISTEN:/s"),
        ("unix-c:/s", "UNIX-CONNECT:/s"),
        ("VSOCK-L:9", "VSOCK-LISTEN:9"),
        ("vsock-c:3:9", "VSOCK-CONNECT:3:9"),
        ("ws-l:0.0.0.0:80", "WS-LISTEN:0.0.0.0:80"),
        ("ws-c:h:80", "WS-CONNECT:h:80"),
        ("wss-l:443", "WSS-LISTEN:443"),
        ("webtransport-c:h:443", "WEBTRANSPORT-CONNECT:h:443"),
        ("TCP-LISTEN:80,FORK", "TCP-LISTEN:80,mux"),
        (
            "TCP-LISTEN:80,Max-Children=2,MUX",
            "TCP-LISTEN:80,mux,max-children=2",
        ),
        ("TCP:Host.Example:1", "TCP-CONNECT:host.example:1"),
    ]);
}

// #57
#[test]
fn prefixes() {
    same(&[
        ("l-tcp:8080", "TCP-LISTEN:8080"),
        ("listen-tcp:8080", "TCP-LISTEN:8080"),
        ("c-tcp:h:1", "TCP-CONNECT:h:1"),
        ("connect-tcp:h:1", "TCP-CONNECT:h:1"),
        ("L-UDP:53", "UDP-LISTEN:53"),
        ("connect-udp:h:53", "UDP-CONNECT:h:53"),
        ("l-ws:0.0.0.0:80", "WS-LISTEN:0.0.0.0:80"),
        ("c-wss:h:443", "WSS-CONNECT:h:443"),
        ("listen-webtransport:443", "WEBTRANSPORT-LISTEN:443"),
        ("listen-unix:/s", "UNIX-LISTEN:/s"),
        ("c-unix:/s", "UNIX-CONNECT:/s"),
        ("c-vsock:3:9", "VSOCK-CONNECT:3:9"),
        ("l-vsock:9", "VSOCK-LISTEN:9"),
        ("l-abstract:n", "ABSTRACT-LISTEN:n"),
        ("connect-abstract:n", "ABSTRACT-CONNECT:n"),
        ("UNIX:/s", "UNIX-CONNECT:/s"),
        ("local:/s", "UNIX-CONNECT:/s"),
    ]);
}

// #57
#[test]
fn listen_host_is_optional() {
    assert_eq!(
        parse("TCP-LISTEN:8080").target(),
        &Target::Net {
            net: Net::Tcp,
            inet: Inet::Listen {
                host: None,
                port: num(8080)
            }
        }
    );
    assert_eq!(
        parse("VSOCK-LISTEN:9000").target(),
        &Target::Vsock(Vsock::Listen { port: 9000 })
    );
}

// #57
#[test]
fn files() {
    assert_eq!(exec("FILE:/x"), Target::File(text("/x")));
    assert_eq!(exec("OPEN:/x"), Target::Open(text("/x")));
    assert_eq!(exec("GOPEN:/x"), Target::Gopen(text("/x")));
    let bare = [
        ("/dev/ttyS0", "/dev/ttyS0"),
        ("/a:b", "/a:b"),
        ("/a:b:c", "/a:b:c"),
        (r"/a\ b", "/a b"),
        ("'/a,b'", "/a,b"),
        ("/a[b:c]", "/a[b:c]"),
        ("/é:ü", "/é:ü"),
        ("./rel", "./rel"),
    ];
    for (s, path) in bare {
        assert_eq!(exec(s), Target::Gopen(text(path)), "{s}");
    }
    same(&[
        ("/dev/ttyS0,RAW", "GOPEN:/dev/ttyS0,raw"),
        ("/a:b", "GOPEN:'/a:b'"),
        ("OPEN:/x", "OPEN:/x"),
        ("FILE:/a:b", "FILE:'/a:b'"),
    ]);
}

// #57
#[test]
fn unix_and_abstract() {
    assert_eq!(
        exec("UNIX-CONNECT:@name"),
        Target::Unix {
            mode: Mode::Connect,
            path: text("@name")
        }
    );
    assert_eq!(
        exec("ABSTRACT-CONNECT:name"),
        Target::Abstract {
            mode: Mode::Connect,
            name: text("name")
        }
    );
    assert!(parse("ABSTRACT-LISTEN:n,mux").options().mux());
    same(&[
        ("UNIX-CONNECT:@name", "UNIX-CONNECT:@name"),
        ("abstract-l:n", "ABSTRACT-LISTEN:n"),
        ("ABSTRACT-C:a:b", "ABSTRACT-CONNECT:'a:b'"),
    ]);
}

// #57
#[test]
fn single() {
    let cases = [
        ("FILE:/dev/ttyS0", true),
        ("OPEN:/x", true),
        ("GOPEN:/x", true),
        ("/dev/ttyUSB0", true),
        ("STDIO", true),
        ("PTY", true),
        ("EXEC:cat", false),
        ("UNIX-CONNECT:/s", false),
        ("UNIX-LISTEN:/s", false),
        ("ABSTRACT-CONNECT:n", false),
        ("ABSTRACT-LISTEN:n", false),
        ("VSOCK-CONNECT:3:1", false),
        ("VSOCK-LISTEN:1", false),
        ("TCP:h:1", false),
        ("TCP-LISTEN:1", false),
        ("UDP:h:1", false),
        ("UDP-LISTEN:1", false),
        ("WS-CONNECT:h:1", false),
        ("WSS-LISTEN:1", false),
        ("WEBTRANSPORT-CONNECT:h:1", false),
    ];
    for (s, want) in cases {
        assert_eq!(parse(s).target().single(), want, "{s}");
    }
}

// #57
#[test]
fn deferred_forms() {
    let keyword = |k: &str| Kind::Keyword(k.into());
    table(&[
        ("TCP4:h:1", 0, keyword("TCP4")),
        ("TCP6:h:1", 0, keyword("TCP6")),
        ("TCP4-LISTEN:80", 0, keyword("TCP4-LISTEN")),
        ("TCP6-LISTEN:443", 0, keyword("TCP6-LISTEN")),
        ("UDP4:h:1", 0, keyword("UDP4")),
        ("UDP-SENDTO:h:1", 0, keyword("UDP-SENDTO")),
        ("UDP-RECVFROM:1", 0, keyword("UDP-RECVFROM")),
        ("UDP-DATAGRAM:h:1", 0, keyword("UDP-DATAGRAM")),
        ("UNIX-SENDTO:/s", 0, keyword("UNIX-SENDTO")),
        ("UNIX-CLIENT:/s", 0, keyword("UNIX-CLIENT")),
        ("ABSTRACT-SENDTO:n", 0, keyword("ABSTRACT-SENDTO")),
        ("OPENSSL:h:443", 0, keyword("OPENSSL")),
        ("OPENSSL-LISTEN:443", 0, keyword("OPENSSL-LISTEN")),
        ("SCTP-LISTEN:1", 0, keyword("SCTP-LISTEN")),
        ("SOCKS4:p:h:1", 0, keyword("SOCKS4")),
        ("PROXY:p:h:1", 0, keyword("PROXY")),
        ("FD:3", 0, keyword("FD")),
        ("3", 0, keyword("3")),
        ("PIPE", 0, keyword("PIPE")),
        ("SYSTEM:ls", 0, keyword("SYSTEM")),
        ("TUN", 0, keyword("TUN")),
        ("C-STDIO", 0, keyword("C-STDIO")),
        ("http://h", 0, keyword("http")),
        ("STDIO!!STDIO", 5, Kind::Dual),
    ]);
}

// #57
#[test]
fn extra_parameters() {
    table(&[
        ("TCP:h:1:2", 8, Kind::Extra("2".into())),
        ("TCP-LISTEN:h:1:2", 15, Kind::Extra("2".into())),
        ("UDP-LISTEN:h:1:2", 15, Kind::Extra("2".into())),
        ("WS-CONNECT:h:1:2", 15, Kind::Extra("2".into())),
        ("WS-LISTEN:h:1:2", 14, Kind::Extra("2".into())),
        (
            "VSOCK-CONNECT:3:9000:garbage",
            21,
            Kind::Extra("garbage".into()),
        ),
        ("VSOCK-LISTEN:1:2", 15, Kind::Extra("2".into())),
        ("STDIO:x", 6, Kind::Extra("x".into())),
        ("PTY:x", 4, Kind::Extra("x".into())),
        ("TCP:h:80:", 9, Kind::Blank),
        ("TCP-LISTEN:80:", 14, Kind::Missing(Part::Port)),
        ("VSOCK-LISTEN:1:", 15, Kind::Blank),
        ("STDIO:", 6, Kind::Blank),
    ]);
}

// #57
#[test]
fn numbers() {
    let vsock = |s: &str| match exec(s) {
        Target::Vsock(v) => v,
        t => panic!("{s}: {t:?}"),
    };
    assert_eq!(vsock("VSOCK-LISTEN:0"), Vsock::Listen { port: 0 });
    assert_eq!(vsock("VSOCK-LISTEN:1"), Vsock::Listen { port: 1 });
    assert_eq!(
        vsock("VSOCK-LISTEN:4294967295"),
        Vsock::Listen { port: u32::MAX }
    );
    assert_eq!(
        vsock("VSOCK-CONNECT:0:0"),
        Vsock::Connect { cid: 0, port: 0 }
    );
    assert_eq!(
        vsock("VSOCK-CONNECT:4294967295:4294967295"),
        Vsock::Connect {
            cid: u32::MAX,
            port: u32::MAX
        }
    );
    let cap = |s: &str| parse(s).options().cap();
    assert_eq!(cap("TCP-LISTEN:80,mux,max-children=1"), NonZeroU32::new(1));
    assert_eq!(
        cap("TCP-LISTEN:80,mux,max-children=4294967295"),
        NonZeroU32::new(u32::MAX)
    );
    same(&[
        ("VSOCK-LISTEN:4294967295", "VSOCK-LISTEN:4294967295"),
        (
            "TCP-LISTEN:80,mux,max-children=4294967295",
            "TCP-LISTEN:80,mux,max-children=4294967295",
        ),
    ]);
    let value = || Kind::Value("max-children".into());
    table(&[
        (
            "VSOCK-LISTEN:4294967296",
            13,
            Kind::Port("4294967296".into()),
        ),
        (
            "VSOCK-CONNECT:4294967296:1",
            14,
            Kind::Cid("4294967296".into()),
        ),
        (
            "VSOCK-CONNECT:1:4294967296",
            16,
            Kind::Port("4294967296".into()),
        ),
        ("VSOCK-LISTEN:010", 13, Kind::Base("010".into())),
        ("VSOCK-LISTEN:0x50", 13, Kind::Base("0x50".into())),
        ("VSOCK-CONNECT:0x2:1", 14, Kind::Base("0x2".into())),
        ("VSOCK-CONNECT:2:00", 16, Kind::Base("00".into())),
        ("VSOCK-LISTEN:00x1", 13, Kind::Base("00x1".into())),
        ("VSOCK-LISTEN:0b1", 13, Kind::Base("0b1".into())),
        ("VSOCK-LISTEN:0x", 13, Kind::Base("0x".into())),
        ("VSOCK-LISTEN:0o7", 13, Kind::Base("0o7".into())),
        (
            "TCP-LISTEN:80,mux,max-children=0b1",
            18,
            Kind::Base("0b1".into()),
        ),
        ("VSOCK-LISTEN:+1", 13, Kind::Port("+1".into())),
        ("VSOCK-LISTEN:-1", 13, Kind::Port("-1".into())),
        ("TCP-LISTEN:80,mux,max-children=0", 18, value()),
        ("TCP-LISTEN:80,mux,max-children=4294967296", 18, value()),
        ("TCP-LISTEN:80,mux,max-children=+2", 18, value()),
        ("TCP-LISTEN:80,mux,max-children=-1", 18, value()),
        (
            "TCP-LISTEN:80,mux,max-children=010",
            18,
            Kind::Base("010".into()),
        ),
        (
            "TCP-LISTEN:80,mux,max-children=0x2",
            18,
            Kind::Base("0x2".into()),
        ),
    ]);
}

proptest! {
    // #57
    #[test]
    fn vsock_out_of_range(p in (u32::MAX as u64 + 1)..) {
        prop_assert_eq!(
            Address::parse(&format!("VSOCK-LISTEN:{p}")),
            Err(Error::new(13, Kind::Port(p.to_string())))
        );
        prop_assert_eq!(
            Address::parse(&format!("VSOCK-CONNECT:{p}:1")),
            Err(Error::new(14, Kind::Cid(p.to_string())))
        );
        prop_assert_eq!(
            Address::parse(&format!("TCP-LISTEN:1,mux,max-children={p}")),
            Err(Error::new(17, Kind::Value("max-children".into())))
        );
    }

    // #57
    #[test]
    fn leading_zero(n in any::<u32>(), p in any::<u16>()) {
        let z = format!("0{n}");
        prop_assert_eq!(
            Address::parse(&format!("VSOCK-LISTEN:{z}")),
            Err(Error::new(13, Kind::Base(z.clone())))
        );
        prop_assert_eq!(
            Address::parse(&format!("TCP:h:0{p}")).map(|a| a.to_string()),
            Ok(format!("TCP-CONNECT:h:{p}"))
        );
    }

    // #57
    #[test]
    fn unknown_keyword(k in "[A-Z][A-Z0-9-]{0,10}") {
        prop_assume!(!keywords().contains(&k));
        prop_assert_eq!(
            Address::parse(&format!("{k}:x")),
            Err(Error::new(0, Kind::Keyword(k.clone())))
        );
    }
}
