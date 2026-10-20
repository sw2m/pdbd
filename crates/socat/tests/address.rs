mod support;

use proptest::prelude::*;
use socat::{
    Address, Host, Inet, Kind, Mode, Name, Net, Opt, Options, Part, Resource,
    Service, Target, Text, Unchecked, Vsock, Web, Zone,
};
use std::net::Ipv4Addr;
use std::num::NonZeroU32;
use support::fixture::{fail, name, num, opt, parse, plain, table, text, web};
use support::strategy::{addresses, check, edit, respell, spelling};

// #57
#[test]
fn transports() {
    let any = Ipv4Addr::UNSPECIFIED;
    let cases = [
        (
            "FILE:/dev/ttyS0,b115200,raw",
            plain(
                Target::File(text("/dev/ttyS0")),
                vec![opt("b115200", None), opt("raw", None)],
            ),
        ),
        (
            "UNIX-CONNECT:/run/vm/guest.sock",
            plain(
                Target::Unix {
                    mode: Mode::Connect,
                    path: text("/run/vm/guest.sock"),
                },
                vec![],
            ),
        ),
        (
            "VSOCK-CONNECT:3:9000",
            plain(Target::Vsock(Vsock::Connect { cid: 3, port: 9000 }), vec![]),
        ),
        (
            "UDP-CONNECT:host:9000",
            plain(
                Target::Net {
                    net: Net::Udp,
                    inet: Inet::Connect {
                        host: name("host"),
                        port: num(9000),
                    },
                },
                vec![],
            ),
        ),
        (
            "WS-LISTEN:0.0.0.0:8080",
            plain(
                web(
                    Web::Ws,
                    Inet::Listen {
                        host: Some(Host::V4(any)),
                        port: num(8080),
                    },
                    "/",
                ),
                vec![],
            ),
        ),
        (
            "WSS-CONNECT:host:443",
            plain(
                web(
                    Web::Wss,
                    Inet::Connect {
                        host: name("host"),
                        port: num(443),
                    },
                    "/",
                ),
                vec![],
            ),
        ),
        (
            "WEBTRANSPORT-CONNECT:host:443",
            plain(
                web(
                    Web::Webtransport,
                    Inet::Connect {
                        host: name("host"),
                        port: num(443),
                    },
                    "/",
                ),
                vec![],
            ),
        ),
        (
            "EXEC:'ssh jump nc target 23'",
            plain(Target::Exec(text("ssh jump nc target 23")), vec![]),
        ),
        ("STDIO", plain(Target::Stdio, vec![])),
    ];
    for (s, want) in cases {
        assert_eq!(parse(s), want, "{s}");
    }
}

// #57
#[test]
fn forms() {
    let cases = [
        ("TCP:127.0.0.1:7000", "TCP-CONNECT:127.0.0.1:7000"),
        ("TCP:10.0.0.5:80", "TCP-CONNECT:10.0.0.5:80"),
        ("TCP-LISTEN:8080", "TCP-LISTEN:8080"),
        ("TCP-LISTEN:8080,mux", "TCP-LISTEN:8080,mux"),
        ("TCP-LISTEN:8080,fork", "TCP-LISTEN:8080,mux"),
        (
            "TCP-LISTEN:8080,fork,max-children=4",
            "TCP-LISTEN:8080,mux,max-children=4",
        ),
        ("UDP-LISTEN:53,mux", "UDP-LISTEN:53,mux"),
        ("FILE:/dev/ttyUSB0,b115200,raw", "FILE:/dev/ttyUSB0,b115200,raw"),
        (
            "WSS-LISTEN:0.0.0.0:443,cert=server.pem,key=server.key",
            "WSS-LISTEN:0.0.0.0:443,cert=server.pem,key=server.key",
        ),
        (
            "WEBTRANSPORT-LISTEN:0.0.0.0:443,cert=server.pem,key=server.key",
            "WEBTRANSPORT-LISTEN:0.0.0.0:443,cert=server.pem,key=server.key",
        ),
        (
            "EXEC:'/usr/local/bin/sensor'",
            "EXEC:/usr/local/bin/sensor",
        ),
        (
            "EXEC:'socat - OPENSSL-LISTEN:4433,reuseaddr,cert=server.pem,key=server.key,verify=1'",
            "EXEC:'socat - OPENSSL-LISTEN:4433,reuseaddr,cert=server.pem,key=server.key,verify=1'",
        ),
        (
            "EXEC:'socat - OPENSSL:gateway.example:4433,verify=1'",
            "EXEC:'socat - OPENSSL:gateway.example:4433,verify=1'",
        ),
        (
            "EXEC:'openssl s_server -quiet -accept 4433 -cert server.pem -key server.key'",
            "EXEC:'openssl s_server -quiet -accept 4433 -cert server.pem -key server.key'",
        ),
        (
            "EXEC:'openssl s_client -quiet -connect gateway.example:4433'",
            "EXEC:'openssl s_client -quiet -connect gateway.example:4433'",
        ),
        (
            "EXEC:'ssh -W dbhost:4000 jump'",
            "EXEC:'ssh -W dbhost:4000 jump'",
        ),
        (
            "EXEC:'ssh host pdbd --stdio'",
            "EXEC:'ssh host pdbd --stdio'",
        ),
        (
            "EXEC:'ncat --proxy 127.0.0.1:1080 --proxy-type socks5 dbhost 4000'",
            "EXEC:'ncat --proxy 127.0.0.1:1080 --proxy-type socks5 dbhost 4000'",
        ),
        (
            "EXEC:'quicat quic-passive-listen://0.0.0.0:4433 stdio'",
            "EXEC:'quicat quic-passive-listen://0.0.0.0:4433 stdio'",
        ),
        (
            "EXEC:'quicat stdio quic-active-connect://gateway.example:4433'",
            "EXEC:'quicat stdio quic-active-connect://gateway.example:4433'",
        ),
        ("PTY", "PTY"),
    ];
    for (s, want) in cases {
        assert_eq!(parse(s).to_string(), want, "{s}");
    }
}

// #57
#[test]
fn positions() {
    table(&[
        ("", 0, Kind::Empty),
        ("BOGUS:1", 0, Kind::Keyword("BOGUS".into())),
        ("TCP-X:h:1", 0, Kind::Keyword("TCP-X".into())),
        ("X-TCP:h:1", 0, Kind::Keyword("X-TCP".into())),
        ("L-L:1", 0, Kind::Keyword("L-L".into())),
        ("WS:h:1", 0, Kind::Keyword("WS".into())),
        ("TCP:host", 8, Kind::Missing(Part::Port)),
        ("TCP", 3, Kind::Missing(Part::Host)),
        ("TCP-LISTEN", 10, Kind::Missing(Part::Port)),
        ("TCP-LISTEN:h:", 13, Kind::Missing(Part::Port)),
        ("TCP::1", 4, Kind::Missing(Part::Host)),
        ("TCP:host:99999", 9, Kind::Port("99999".into())),
        ("TCP:host:+65536", 9, Kind::Port("+65536".into())),
        ("TCP:host:-1", 9, Kind::Port("-1".into())),
        ("TCP:host:+", 9, Kind::Port("+".into())),
        ("TCP:host:-x", 9, Kind::Port("-x".into())),
        ("TCP:[::1]x:1", 4, Kind::Host("[::1]x".into())),
        ("TCP:[::g]:1", 4, Kind::Host("[::g]".into())),
        ("TCP:[fe80::1%]:1", 4, Kind::Host("[fe80::1%]".into())),
        ("TCP:[fe80::1%a/b]:1", 4, Kind::Host("[fe80::1%a/b]".into())),
        ("TCP:[1.2.3.4]:1", 4, Kind::Host("[1.2.3.4]".into())),
        ("TCP:'a b':1", 4, Kind::Host("a b".into())),
        ("TCP:a/b:1", 4, Kind::Host("a/b".into())),
        ("TCP:a!b:1", 4, Kind::Host("a!b".into())),
        ("VSOCK-CONNECT:x:1", 14, Kind::Cid("x".into())),
        ("VSOCK-CONNECT:3", 15, Kind::Missing(Part::Port)),
        ("VSOCK-CONNECT", 13, Kind::Missing(Part::Cid)),
        ("EXEC:'abc", 5, Kind::Quote),
        ("EXEC:\"abc", 5, Kind::Quote),
        ("EXEC:abc\\", 8, Kind::Escape),
        ("EXEC", 4, Kind::Missing(Part::Command)),
        ("EXEC:", 5, Kind::Missing(Part::Command)),
        ("FILE:", 5, Kind::Missing(Part::Path)),
        ("OPEN", 4, Kind::Missing(Part::Path)),
        ("UNIX-CONNECT", 12, Kind::Missing(Part::Path)),
        ("ABSTRACT-CONNECT", 16, Kind::Missing(Part::Name)),
        ("TCP:[::1:80", 4, Kind::Bracket),
        ("EXEC:a[b", 6, Kind::Bracket),
        ("EXEC:(a]", 7, Kind::Bracket),
        ("TCP-LISTEN:80,=x", 14, Kind::Option("=x".into())),
        ("TCP-LISTEN:80,a b", 14, Kind::Option("a b".into())),
        ("TCP-CONNECT:h:1,mux", 16, Kind::Option("mux".into())),
        ("EXEC:x,fork", 7, Kind::Option("fork".into())),
        (
            "STDIO,max-children=2",
            6,
            Kind::Option("max-children".into()),
        ),
        ("TCP-LISTEN:80,mux=3", 14, Kind::Value("mux".into())),
        (
            "TCP-LISTEN:80,mux,max-children=x",
            18,
            Kind::Value("max-children".into()),
        ),
        (
            "TCP-LISTEN:80,mux,max-children",
            18,
            Kind::Value("max-children".into()),
        ),
        ("TCP-LISTEN:80,mux,fork", 18, Kind::Duplicate("mux".into())),
        ("TCP-LISTEN:80,max-children=2", 14, Kind::Cap),
        ("WS-CONNECT:h:80/a b", 17, Kind::Resource("/a b".into())),
        ("WS-CONNECT:h:80/x#f", 17, Kind::Resource("/x#f".into())),
        ("ws://h/x#f", 8, Kind::Resource("/x#f".into())),
        ("ws://h#f", 6, Kind::Resource("#f".into())),
        ("ws://h:80/a b", 11, Kind::Resource("/a b".into())),
        ("ws://", 5, Kind::Missing(Part::Host)),
        ("ws://h:99999/", 7, Kind::Port("99999".into())),
        ("ws:x", 0, Kind::Keyword("ws".into())),
        ("EXEC:é\\", 7, Kind::Escape),
        ("EXEC:(é]", 8, Kind::Bracket),
        ("EXEC:é[x", 7, Kind::Bracket),
        ("EXEC:é'x", 7, Kind::Quote),
        ("TCP:h:é", 6, Kind::Port("é".into())),
        ("TCP:h:1:é", 8, Kind::Extra("é".into())),
        ("STDIO,é", 6, Kind::Option("é".into())),
        ("FILE:é,x=é,mux", 13, Kind::Option("mux".into())),
        ("VSOCK-LISTEN:é", 13, Kind::Port("é".into())),
        ("TCP:'é b':1", 4, Kind::Host("é b".into())),
    ]);
}

// #57
#[test]
fn display() {
    assert_eq!(
        fail("TCP:host:99999").to_string(),
        "at byte 9: invalid port \"99999\""
    );
    assert_eq!(fail("TCP:host").to_string(), "at byte 8: missing port");
    assert_eq!(
        fail("STDIO!!STDIO").to_string(),
        "at byte 5: dual address \"!!\" where one address is expected"
    );
}

// #57
#[test]
fn invariants() {
    assert_eq!(Text::new(""), None);
    assert!(Text::new("x").is_some());
    for bad in ["", "a,b", "a:b", "a b", "a!b", "1.2.3.4", "[x]", "a\nb"] {
        assert_eq!(Name::new(bad), None, "{bad:?}");
    }
    assert_eq!(Name::new("A.b").expect("name").as_str(), "a.b");
    #[cfg(feature = "idn")]
    assert_eq!(Name::new("Bü").expect("name").as_str(), "bü");
    for bad in ["", "a%b", "a]b", "a b"] {
        assert_eq!(Zone::new(bad), None, "{bad:?}");
    }
    for bad in ["", "80", "+x", "-x", "a b", "é"] {
        assert_eq!(Service::new(bad), None, "{bad:?}");
    }
    assert!(Service::new("http").is_some());
    for bad in ["", "x", "/a b", "/a\nb", "/x#f"] {
        assert_eq!(Resource::new(bad), None, "{bad:?}");
    }
    assert!(Resource::new("/a?b=1").is_some());
    for bad in ["", "a b", "a,b", "a=b", "mux", "FORK", "max_children"] {
        assert_eq!(Opt::new(bad, None), None, "{bad:?}");
    }
    assert_eq!(opt("RAW", None).name(), "raw");
    let cap = NonZeroU32::new(1);
    assert_eq!(Options::new(false, cap, Unchecked::default()), None);
    let mux = Options::new(true, cap, Unchecked::default()).expect("options");
    assert_eq!(
        Address::new(Target::Stdio, mux.clone()),
        Err(Kind::Option("mux".into()))
    );
    let listen = Target::Net {
        net: Net::Tcp,
        inet: Inet::Listen {
            host: None,
            port: num(80),
        },
    };
    assert_eq!(
        Address::new(listen, mux).expect("address").to_string(),
        "TCP-LISTEN:80,mux,max-children=1"
    );
}

proptest! {
    #![proptest_config(support::strategy::config())]

    // #57
    #[test]
    fn roundtrip(a in addresses()) {
        let s = a.to_string();
        prop_assert_eq!(Address::parse(&s), Ok(a), "{:?}", s);
    }

    // #57
    #[test]
    fn canonical(a in addresses(), sp in spelling()) {
        let s = respell(&a, &sp);
        let parsed = Address::parse(&s);
        prop_assert_eq!(
            parsed.as_ref().map(ToString::to_string),
            Ok(a.to_string()),
            "{:?}",
            s
        );
        prop_assert_eq!(parsed, Ok(a), "{:?}", s);
    }

    // #57
    #[test]
    fn printable(s in "[ -~]{0,40}") {
        check(&s)?;
    }

    // #57
    #[test]
    fn unicode(s in "\\PC{0,24}") {
        check(&s)?;
    }

    // #57
    #[test]
    fn mutated(a in addresses(), e in edit()) {
        match String::from_utf8(e.apply(&a.to_string())) {
            Ok(s) => check(&s)?,
            Err(bad) => check(&String::from_utf8_lossy(bad.as_bytes()))?,
        }
    }
}
