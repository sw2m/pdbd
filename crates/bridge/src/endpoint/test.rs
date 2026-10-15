use super::*;
use proptest::prelude::*;
use socat::{Mode, Net, Target};

fn sock(host: &str, port: u16) -> Socket {
    Socket {
        host: Some(host.into()),
        port,
    }
}

fn bridge(a: &str, b: &str) -> Bridge {
    Bridge::parse(a, b).unwrap_or_else(|e| panic!("{a} {b}: {e}"))
}

fn fail(s: &str) -> Error {
    match Endpoint::parse(s) {
        Ok(e) => panic!("{s:?} parsed as {e:?}"),
        Err(e) => e,
    }
}

fn err(arg: usize, at: usize, kind: Kind) -> Error {
    Error { arg, at, kind }
}

// #19 — README §"The `pdb` / `pdbd` CLI": local-forward
#[test]
fn alias_forward() {
    let b = bridge("client:bind:0.0.0.0:5432", "daemon:forward:10.0.0.5:5432");
    assert_eq!(
        b,
        Bridge {
            client: Spec::Alias(Alias::Bind, sock("0.0.0.0", 5432)),
            daemon: Spec::Alias(Alias::Forward, sock("10.0.0.5", 5432)),
        }
    );
    assert!(b.framed());
    assert_eq!(b.cap(), None);
    assert_eq!(b.ingress(), Some(Side::Client));
}

// #19 — reverse: swap the sides
#[test]
fn alias_reverse() {
    let b = bridge("daemon:bind:0.0.0.0:8080", "client:forward:127.0.0.1:3000");
    assert_eq!(
        b,
        Bridge {
            client: Spec::Alias(Alias::Forward, sock("127.0.0.1", 3000)),
            daemon: Spec::Alias(Alias::Bind, sock("0.0.0.0", 8080)),
        }
    );
    assert_eq!(b.ingress(), Some(Side::Daemon));
}

// #19 — a raw LISTEN bridges one connection: a bare stream, no frame
#[test]
fn raw_single() {
    let b = bridge(
        "client:TCP-LISTEN:8080",
        "daemon:EXEC:'/usr/local/bin/sensor'",
    );
    assert!(!b.framed());
    assert_eq!(b.ingress(), Some(Side::Client));
    assert_eq!(
        b.daemon,
        Spec::Raw(Address {
            target: Target::Exec("/usr/local/bin/sensor".into()),
            options: vec![],
        })
    );
}

// #19 — `,mux` (≡ socat `,fork`) multiplexes; max-children is the cap
#[test]
fn raw_mux() {
    for opt in ["mux", "fork"] {
        let b = bridge(
            &format!("client:TCP-LISTEN:8080,{opt}"),
            "daemon:TCP:10.0.0.5:80",
        );
        assert!(b.framed(), "{opt}");
        assert_eq!(b.cap(), None);
    }
    let b = bridge(
        "daemon:UDP-LISTEN:53,fork,max-children=8",
        "client:UDP:h:53",
    );
    assert!(b.framed());
    assert_eq!(b.cap(), Some(8));
    assert_eq!(b.ingress(), Some(Side::Daemon));
}

// #19 — a raw bridge with no listener (or two) has no single ingress
#[test]
fn raw_ingress() {
    assert_eq!(bridge("client:STDIO", "daemon:TCP:h:1").ingress(), None);
    assert_eq!(
        bridge("client:TCP-LISTEN:1", "daemon:TCP-LISTEN:2").ingress(),
        None
    );
}

// #19 — sides are case-sensitive tokens (README §"Parsing rule")
#[test]
fn endpoint_errors() {
    let cases: &[(&str, usize, Kind)] = &[
        ("server:bind:1.2.3.4:5", 0, Kind::Side("server".into())),
        ("Client:bind:1.2.3.4:5", 0, Kind::Side("Client".into())),
        ("client", 6, Kind::Missing),
        ("client:", 7, Kind::Missing),
        ("client:bind:tcp:1.2.3.4:80", 12, Kind::Proto("tcp".into())),
        ("daemon:forward:UDP:h:80", 15, Kind::Proto("UDP".into())),
        (
            "client:bind:1.2.3.4:80,mux",
            23,
            Kind::Address(socat::Kind::Option("mux".into())),
        ),
        (
            "client:bind:1.2.3.4",
            19,
            Kind::Address(socat::Kind::Missing("port")),
        ),
        (
            "client:forward:h:1:2",
            19,
            Kind::Address(socat::Kind::Extra("2".into())),
        ),
        (
            "client:TCP:h:99999",
            13,
            Kind::Address(socat::Kind::Port("99999".into())),
        ),
        (
            "daemon:NOPE:1",
            7,
            Kind::Address(socat::Kind::Keyword("NOPE".into())),
        ),
    ];
    for (s, at, kind) in cases {
        assert_eq!(fail(s), err(0, *at, kind.clone()), "{s:?}");
    }
}

// #19 — exactly one bind + one forward, one end per side
#[test]
fn pair_errors() {
    let cases: &[(&str, &str, Error)] = &[
        (
            "client:bind:h:1",
            "client:forward:h:2",
            err(1, 0, Kind::Twice(Side::Client)),
        ),
        (
            "client:bind:h:1",
            "daemon:bind:h:2",
            err(1, 0, Kind::Same(Alias::Bind)),
        ),
        (
            "client:forward:h:1",
            "daemon:forward:h:2",
            err(1, 0, Kind::Same(Alias::Forward)),
        ),
        ("client:bind:h:1", "daemon:TCP:h:2", err(1, 0, Kind::Mixed)),
        (
            "client:TCP-LISTEN:1",
            "daemon:forward:h:2",
            err(1, 0, Kind::Mixed),
        ),
        (
            "client:bind:h:1",
            "daemon:TCP:h:99999",
            err(1, 13, Kind::Address(socat::Kind::Port("99999".into()))),
        ),
    ];
    for (a, b, want) in cases {
        assert_eq!(Bridge::parse(a, b).as_ref(), Err(want), "{a} {b}");
    }
}

// #19
#[test]
fn error_display_names_the_argument() {
    assert_eq!(
        Bridge::parse("client:bind:h:1", "daemon:TCP:h:99999")
            .unwrap_err()
            .to_string(),
        "endpoint 2, at byte 13: invalid port \"99999\""
    );
}

// #19
#[test]
fn print() {
    let cases = [
        ("client:bind:0.0.0.0:5432", "client:bind:0.0.0.0:5432"),
        ("daemon:forward:[::1]:80", "daemon:forward:[::1]:80"),
        ("client:tcp-l:8080,FORK", "client:TCP-LISTEN:8080,fork"),
        ("daemon:-", "daemon:STDIO"),
    ];
    for (s, want) in cases {
        let e = Endpoint::parse(s).unwrap();
        assert_eq!(e.to_string(), want);
    }
}

fn side() -> impl Strategy<Value = Side> {
    prop_oneof![Just(Side::Client), Just(Side::Daemon)]
}

fn host() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z][a-z0-9.-]{0,15}",
        any::<[u16; 8]>().prop_map(|s| std::net::Ipv6Addr::from(s).to_string()),
    ]
}

fn socket() -> impl Strategy<Value = Socket> {
    (host(), any::<u16>()).prop_map(|(h, port)| Socket {
        host: Some(h),
        port,
    })
}

fn raw() -> impl Strategy<Value = Address> {
    (socket(), any::<bool>(), any::<bool>()).prop_map(
        |(socket, listen, mux)| {
            let mode = if listen { Mode::Listen } else { Mode::Connect };
            let options = if listen && mux {
                vec![socat::Opt {
                    name: "mux".into(),
                    value: None,
                }]
            } else {
                vec![]
            };
            Address {
                target: Target::Net {
                    net: Net::Tcp,
                    mode,
                    socket,
                },
                options,
            }
        },
    )
}

fn spec() -> impl Strategy<Value = Spec> {
    prop_oneof![
        (
            prop_oneof![Just(Alias::Bind), Just(Alias::Forward)],
            socket()
        )
            .prop_map(|(a, s)| Spec::Alias(a, s)),
        raw().prop_map(Spec::Raw),
    ]
}

proptest! {
    // #19 — parse ∘ print is the identity on endpoints
    #[test]
    fn roundtrip(side in side(), spec in spec()) {
        let e = Endpoint { side, spec };
        prop_assert_eq!(Endpoint::parse(&e.to_string()), Ok(e));
    }

    // #19 — a valid alias pair parses the same in either order
    #[test]
    fn either_order(
        bind in socket(),
        forward in socket(),
        ingress in side(),
    ) {
        let egress = match ingress {
            Side::Client => Side::Daemon,
            Side::Daemon => Side::Client,
        };
        let a = Endpoint { side: ingress, spec: Spec::Alias(Alias::Bind, bind) };
        let b = Endpoint { side: egress, spec: Spec::Alias(Alias::Forward, forward) };
        let (a, b) = (a.to_string(), b.to_string());
        let one = Bridge::parse(&a, &b).unwrap();
        prop_assert_eq!(&one, &Bridge::parse(&b, &a).unwrap());
        prop_assert!(one.framed());
        prop_assert_eq!(one.ingress(), Some(ingress));
    }

    // #19 — two ends on one side never make a bridge
    #[test]
    fn same_side_rejected(side in side(), a in spec(), b in spec()) {
        let a = Endpoint { side, spec: a }.to_string();
        let b = Endpoint { side, spec: b }.to_string();
        prop_assert_eq!(
            Bridge::parse(&a, &b).map_err(|e| e.kind),
            Err(Kind::Twice(side))
        );
    }

    // #19 — arbitrary input never panics, and anything accepted re-prints
    #[test]
    fn arbitrary_input(s in "(client|daemon|x):[ -~]{0,30}") {
        if let Ok(e) = Endpoint::parse(&s) {
            prop_assert_eq!(Endpoint::parse(&e.to_string()), Ok(e));
        }
    }
}
