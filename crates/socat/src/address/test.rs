use super::*;
use proptest::prelude::*;
use std::net::Ipv6Addr;

fn parse(s: &str) -> Address {
    Address::parse(s).unwrap_or_else(|e| panic!("{s:?}: {e}"))
}

fn fail(s: &str) -> Error {
    match Address::parse(s) {
        Ok(a) => panic!("{s:?} parsed as {a:?}"),
        Err(e) => e,
    }
}

fn opt(name: &str, value: Option<&str>) -> Opt {
    Opt {
        name: name.into(),
        value: value.map(Into::into),
    }
}

fn net(net: Net, mode: Mode, host: Option<&str>, port: u16) -> Target {
    Target::Net {
        net,
        mode,
        socket: Socket {
            host: host.map(Into::into),
            port,
        },
    }
}

// #19 — README §"L1 — the transport"
#[test]
fn readme_transports_parse() {
    let cases = [
        (
            "FILE:/dev/ttyS0,b115200,raw",
            Target::File("/dev/ttyS0".into()),
            vec![opt("b115200", None), opt("raw", None)],
        ),
        (
            "UNIX-CONNECT:/run/vm/guest.sock",
            Target::Unix {
                mode: Mode::Connect,
                path: "/run/vm/guest.sock".into(),
            },
            vec![],
        ),
        (
            "VSOCK-CONNECT:3:9000",
            Target::Vsock {
                mode: Mode::Connect,
                cid: Some(3),
                port: 9000,
            },
            vec![],
        ),
        (
            "UDP-CONNECT:host:9000",
            net(Net::Udp, Mode::Connect, Some("host"), 9000),
            vec![],
        ),
        (
            "WS-LISTEN:0.0.0.0:8080",
            net(Net::Ws, Mode::Listen, Some("0.0.0.0"), 8080),
            vec![],
        ),
        (
            "WSS-CONNECT:host:443",
            net(Net::Wss, Mode::Connect, Some("host"), 443),
            vec![],
        ),
        (
            "WEBTRANSPORT-CONNECT:host:443",
            net(Net::Webtransport, Mode::Connect, Some("host"), 443),
            vec![],
        ),
        (
            "EXEC:'ssh jump nc target 23'",
            Target::Exec("ssh jump nc target 23".into()),
            vec![],
        ),
        ("STDIO", Target::Stdio, vec![]),
        (
            "TCP:host:7000",
            net(Net::Tcp, Mode::Connect, Some("host"), 7000),
            vec![],
        ),
    ];
    for (s, target, options) in cases {
        assert_eq!(parse(s), Address { target, options }, "{s}");
    }
}

// #19 — socat(1): keywords are case-insensitive, with synonyms
#[test]
fn synonyms_and_case() {
    assert_eq!(parse("-"), parse("STDIO"));
    assert_eq!(parse("stdio"), parse("STDIO"));
    assert_eq!(parse("tcp-l:80"), parse("TCP-LISTEN:80"));
    assert_eq!(parse("TCP-CONNECT:h:1"), parse("TCP:h:1"));
    assert_eq!(parse("UDP:h:1"), parse("UDP-CONNECT:h:1"));
    assert_eq!(parse("udp-l:53"), parse("UDP-LISTEN:53"));
    assert_eq!(parse("unix-l:/s"), parse("UNIX-LISTEN:/s"));
    assert_eq!(parse("VSOCK-L:9"), parse("VSOCK-LISTEN:9"));
    assert_eq!(parse("ws-l:0.0.0.0:80"), parse("WS-LISTEN:0.0.0.0:80"));
    assert_eq!(parse("ws-c:h:80"), parse("WS-CONNECT:h:80"));
    assert_eq!(parse("OPEN:/dev/ttyS0"), parse("FILE:/dev/ttyS0"));
    assert_eq!(parse("GOPEN:/dev/ttyS0"), parse("FILE:/dev/ttyS0"));
    assert_eq!(parse("/dev/ttyS0,RAW"), parse("FILE:/dev/ttyS0,raw"));
}

// #19
#[test]
fn listen_host_is_optional() {
    assert_eq!(
        parse("TCP-LISTEN:8080").target,
        net(Net::Tcp, Mode::Listen, None, 8080)
    );
    assert_eq!(
        parse("VSOCK-LISTEN:9000").target,
        Target::Vsock {
            mode: Mode::Listen,
            cid: None,
            port: 9000
        }
    );
}

// #19 — socat(1): IPv6 hosts are bracketed
#[test]
fn ipv6_brackets() {
    assert_eq!(
        parse("TCP:[::1]:80").target,
        net(Net::Tcp, Mode::Connect, Some("::1"), 80)
    );
    assert_eq!(parse("TCP:[::1]:80").to_string(), "TCP:[::1]:80");
}

// #19 — an unquoted command or path keeps its colons
#[test]
fn rest_keeps_colons() {
    assert_eq!(
        parse("EXEC:ssh -W host:22 jump").target,
        Target::Exec("ssh -W host:22 jump".into())
    );
    assert_eq!(parse("FILE:/a:b").target, Target::File("/a:b".into()));
}

// #19
#[test]
fn quoting() {
    assert_eq!(
        parse(r#"EXEC:"say \"hi\", ok""#).target,
        Target::Exec(r#"say "hi", ok"#.into())
    );
    assert_eq!(parse(r"EXEC:a\,b").target, Target::Exec("a,b".into()));
    assert_eq!(parse(r"EXEC:'it'\''s'").target, Target::Exec("it's".into()));
    assert_eq!(
        parse("FILE:/x,mode='a,b'").options,
        vec![opt("mode", Some("a,b"))]
    );
}

// #19 — README §"Control plane": `,mux`, its alias `,fork`, max-children
#[test]
fn mux_and_cap() {
    assert!(!parse("TCP-LISTEN:8080").mux());
    assert!(parse("TCP-LISTEN:8080,mux").mux());
    assert!(parse("TCP-LISTEN:8080,fork").mux());
    assert!(parse("UDP-LISTEN:53,FORK").mux());
    assert_eq!(parse("TCP-LISTEN:80,mux").cap(), None);
    assert_eq!(parse("TCP-LISTEN:80,fork,max-children=4").cap(), Some(4));
    assert_eq!(parse("TCP-LISTEN:80").mode(), Some(Mode::Listen));
    assert_eq!(parse("TCP:h:80").mode(), Some(Mode::Connect));
    assert_eq!(parse("EXEC:x").mode(), None);
}

// #19
#[test]
fn canonical_print() {
    let cases = [
        ("tcp-l:80,FORK", "TCP-LISTEN:80,fork"),
        ("-", "STDIO"),
        ("/dev/ttyS0,b115200", "FILE:/dev/ttyS0,b115200"),
        ("EXEC:ssh jump nc target 23", "EXEC:'ssh jump nc target 23'"),
        ("TCP-CONNECT:h:1", "TCP:h:1"),
        ("UDP-CONNECT:h:1", "UDP:h:1"),
        ("VSOCK-CONNECT:3:9000", "VSOCK-CONNECT:3:9000"),
        ("ws-l:0.0.0.0:80", "WS-LISTEN:0.0.0.0:80"),
    ];
    for (s, want) in cases {
        assert_eq!(parse(s).to_string(), want, "{s}");
    }
}

// #19 — precise errors, byte offset into the address
#[test]
fn errors_carry_positions() {
    let cases: &[(&str, usize, Kind)] = &[
        ("", 0, Kind::Empty),
        ("BOGUS:1", 0, Kind::Keyword("BOGUS".into())),
        ("TCP:host", 8, Kind::Missing("port")),
        ("TCP", 3, Kind::Missing("host")),
        ("TCP-LISTEN", 10, Kind::Missing("port")),
        ("TCP:host:99999", 9, Kind::Port("99999".into())),
        ("TCP:host:-1", 9, Kind::Port("-1".into())),
        ("TCP:host:1:2", 11, Kind::Extra("2".into())),
        ("TCP:[::1]x:1", 4, Kind::Bracket),
        ("TCP::1", 4, Kind::Missing("host")),
        ("TCP:'a b':1", 4, Kind::Host("a b".into())),
        ("VSOCK-CONNECT:x:1", 14, Kind::Cid("x".into())),
        ("VSOCK-CONNECT:3", 15, Kind::Missing("port")),
        ("EXEC:'abc", 5, Kind::Quote),
        ("EXEC:\"abc", 5, Kind::Quote),
        ("EXEC:abc\\", 8, Kind::Escape),
        ("EXEC", 4, Kind::Missing("command")),
        ("FILE:", 5, Kind::Missing("path")),
        ("TCP:[::1:80", 4, Kind::Bracket),
        ("EXEC:a]b", 6, Kind::Bracket),
        ("STDIO:x", 6, Kind::Extra("x".into())),
        ("TCP-LISTEN:80,=x", 14, Kind::Option("=x".into())),
        ("TCP-LISTEN:80,,raw", 14, Kind::Option("".into())),
        ("TCP:h:1,mux", 8, Kind::Option("mux".into())),
        ("EXEC:x,fork", 7, Kind::Option("fork".into())),
        ("TCP-LISTEN:80,mux=3", 14, Kind::Value("mux".into())),
        (
            "TCP-LISTEN:80,max-children=x",
            14,
            Kind::Value("max-children".into()),
        ),
        (
            "TCP-LISTEN:80,max-children",
            14,
            Kind::Value("max-children".into()),
        ),
    ];
    for (s, at, kind) in cases {
        assert_eq!(fail(s), Error::new(*at, kind.clone()), "{s:?}");
    }
}

// #19
#[test]
fn error_display_names_the_position() {
    assert_eq!(
        fail("TCP:host:99999").to_string(),
        "at byte 9: invalid port \"99999\""
    );
}

fn host() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z][a-z0-9.-]{0,15}",
        any::<[u16; 8]>().prop_map(|s| Ipv6Addr::from(s).to_string()),
    ]
}

fn text() -> impl Strategy<Value = String> {
    "[ -~]{1,24}"
}

fn mode() -> impl Strategy<Value = Mode> {
    prop_oneof![Just(Mode::Connect), Just(Mode::Listen)]
}

fn socket(mode: Mode) -> BoxedStrategy<Socket> {
    let host = match mode {
        Mode::Connect => host().prop_map(Some).boxed(),
        Mode::Listen => proptest::option::of(host()).boxed(),
    };
    (host, any::<u16>())
        .prop_map(|(host, port)| Socket { host, port })
        .boxed()
}

fn target() -> impl Strategy<Value = Target> {
    let nets = prop_oneof![
        Just(Net::Tcp),
        Just(Net::Udp),
        Just(Net::Ws),
        Just(Net::Wss),
        Just(Net::Webtransport),
    ];
    prop_oneof![
        text().prop_map(Target::File),
        (mode(), text()).prop_map(|(mode, path)| Target::Unix { mode, path }),
        (mode(), any::<u32>(), any::<u32>()).prop_map(|(mode, cid, port)| {
            Target::Vsock {
                mode,
                cid: (mode == Mode::Connect).then_some(cid),
                port,
            }
        }),
        (nets, mode()).prop_flat_map(|(net, mode)| socket(mode)
            .prop_map(move |socket| Target::Net { net, mode, socket })),
        text().prop_map(Target::Exec),
        Just(Target::Stdio),
    ]
}

fn options(listen: bool) -> BoxedStrategy<Vec<Opt>> {
    let plain = ("[a-z][a-z0-9-]{0,10}", proptest::option::of("[ -~]{0,10}"))
        .prop_filter("reserved", |(n, _)| {
            !matches!(n.as_str(), "mux" | "fork" | "max-children")
        })
        .prop_map(|(name, value)| Opt { name, value });
    let mut all = prop::collection::vec(plain.boxed(), 0..4).boxed();
    if listen {
        all = (
            all,
            prop::option::of(prop_oneof!["mux", "fork"]),
            any::<Option<u32>>(),
        )
            .prop_map(|(mut v, mux, cap)| {
                if let Some(m) = mux {
                    v.push(Opt {
                        name: m,
                        value: None,
                    });
                }
                if let Some(c) = cap {
                    v.push(Opt {
                        name: "max-children".into(),
                        value: Some(c.to_string()),
                    });
                }
                v
            })
            .boxed();
    }
    all
}

fn address() -> impl Strategy<Value = Address> {
    target().prop_flat_map(|target| {
        let listen = matches!(
            target,
            Target::Unix {
                mode: Mode::Listen,
                ..
            } | Target::Vsock {
                mode: Mode::Listen,
                ..
            } | Target::Net {
                mode: Mode::Listen,
                ..
            }
        );
        options(listen).prop_map(move |options| Address {
            target: target.clone(),
            options,
        })
    })
}

proptest! {
    // #19 — parse ∘ print is the identity on typed addresses
    #[test]
    fn roundtrip(a in address()) {
        let s = a.to_string();
        prop_assert_eq!(Address::parse(&s), Ok(a), "{}", s);
    }

    // #19 — print is canonical: reprinting a parsed print is stable
    #[test]
    fn print_is_canonical(a in address()) {
        let s = a.to_string();
        prop_assert_eq!(parse(&s).to_string(), s);
    }

    // #19 — arbitrary input never panics, and anything accepted re-prints
    #[test]
    fn arbitrary_input(s in "[ -~]{0,40}") {
        if let Ok(a) = Address::parse(&s) {
            prop_assert_eq!(Address::parse(&a.to_string()), Ok(a));
        }
    }

    // #19 — an out-of-range port is rejected at the port's offset
    #[test]
    fn port_out_of_range(h in "[a-z]{1,8}", p in 65536u32..) {
        let s = format!("TCP:{h}:{p}");
        prop_assert_eq!(
            Address::parse(&s),
            Err(Error::new(5 + h.len(), Kind::Port(p.to_string())))
        );
    }

    // #19 — an unterminated quote is rejected at the quote's offset
    #[test]
    fn unterminated_quote(a in address(), body in "[^']{0,8}") {
        let s = format!("{a},x='{body}");
        let at = s.len() - body.len() - 1;
        prop_assert_eq!(Address::parse(&s), Err(Error::new(at, Kind::Quote)));
    }
}
