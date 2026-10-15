use super::*;
use crate::{Host, Inet, Kind, Net, Opt, Part, Vsock};
use proptest::prelude::*;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::num::NonZeroU32;

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

fn name(n: &str) -> Host {
    Host::Name(n.into())
}

fn connect(net: Net, host: Host, port: u16) -> Target {
    Target::Net {
        net,
        inet: Inet::Connect { host, port },
    }
}

fn listen(net: Net, host: Option<Host>, port: u16) -> Target {
    Target::Net {
        net,
        inet: Inet::Listen { host, port },
    }
}

fn plain(target: Target, rest: Vec<Opt>) -> Address {
    Address {
        target,
        options: Options {
            rest,
            ..Options::default()
        },
    }
}

// #57 — README §"L1 — the transport" `--socket` block
#[test]
fn readme_transports() {
    let any = Ipv4Addr::UNSPECIFIED;
    let cases = [
        (
            "FILE:/dev/ttyS0,b115200,raw",
            plain(
                Target::File("/dev/ttyS0".into()),
                vec![opt("b115200", None), opt("raw", None)],
            ),
        ),
        (
            "UNIX-CONNECT:/run/vm/guest.sock",
            plain(
                Target::Unix {
                    mode: Mode::Connect,
                    path: "/run/vm/guest.sock".into(),
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
            plain(connect(Net::Udp, name("host"), 9000), vec![]),
        ),
        (
            "WS-LISTEN:0.0.0.0:8080",
            plain(listen(Net::Ws, Some(Host::V4(any)), 8080), vec![]),
        ),
        (
            "WSS-CONNECT:host:443",
            plain(connect(Net::Wss, name("host"), 443), vec![]),
        ),
        (
            "WEBTRANSPORT-CONNECT:host:443",
            plain(connect(Net::Webtransport, name("host"), 443), vec![]),
        ),
        (
            "EXEC:'ssh jump nc target 23'",
            plain(Target::Exec("ssh jump nc target 23".into()), vec![]),
        ),
        ("STDIO", plain(Target::Stdio, vec![])),
    ];
    for (s, want) in cases {
        assert_eq!(parse(s), want, "{s}");
    }
}

// #57 — every other address form the README names, with its canonical print
#[test]
fn readme_forms() {
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

// #57 — socat(1): keywords and option names are case-insensitive, with synonyms
#[test]
fn synonyms_and_case() {
    let same = [
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
        ("OPEN:/dev/ttyS0", "FILE:/dev/ttyS0"),
        ("GOPEN:/dev/ttyS0", "FILE:/dev/ttyS0"),
        ("/dev/ttyS0,RAW", "FILE:/dev/ttyS0,raw"),
        ("TCP-LISTEN:80,FORK", "TCP-LISTEN:80,mux"),
        (
            "TCP-LISTEN:80,Max-Children=2,MUX",
            "TCP-LISTEN:80,mux,max-children=2",
        ),
        ("TCP:Host.Example:1", "TCP-CONNECT:host.example:1"),
    ];
    for (s, canonical) in same {
        assert_eq!(parse(s), parse(canonical), "{s}");
        assert_eq!(parse(s).to_string(), canonical, "{s}");
    }
}

// #57
#[test]
fn listen_host_is_optional() {
    assert_eq!(
        parse("TCP-LISTEN:8080").target,
        listen(Net::Tcp, None, 8080)
    );
    assert_eq!(
        parse("VSOCK-LISTEN:9000").target,
        Target::Vsock(Vsock::Listen { port: 9000 })
    );
}

// #57 — socat(1): IPv6 hosts are bracketed; printing compresses them
#[test]
fn ipv6_brackets() {
    assert_eq!(
        parse("TCP:[::1]:80").target,
        connect(Net::Tcp, Host::V6(Ipv6Addr::LOCALHOST), 80)
    );
    assert_eq!(
        parse("TCP:[0:0:0::1]:80").to_string(),
        "TCP-CONNECT:[::1]:80"
    );
    assert_eq!(
        parse("TCP-LISTEN:[::]:80").to_string(),
        "TCP-LISTEN:[::]:80"
    );
}

// #57 — an unquoted command or path keeps its colons
#[test]
fn rest_keeps_colons() {
    assert_eq!(
        parse("EXEC:ssh -W host:22 jump").target,
        Target::Exec("ssh -W host:22 jump".into())
    );
    assert_eq!(parse("FILE:/a:b").target, Target::File("/a:b".into()));
    assert_eq!(parse("FILE:/a:b").to_string(), "FILE:'/a:b'");
}

// #57 — socat(1) ≥1.5.0 lexical rules
#[test]
fn quoting() {
    assert_eq!(
        parse(r#"EXEC:"say \"hi\", ok""#).target,
        Target::Exec(r#"say "hi", ok"#.into())
    );
    assert_eq!(parse(r"EXEC:a\,b").target, Target::Exec("a,b".into()));
    assert_eq!(parse(r"EXEC:'it'\''s'").target, Target::Exec("it's".into()));
    assert_eq!(parse("EXEC:it\\'s").to_string(), r"EXEC:'it'\''s'");
    assert_eq!(
        parse("FILE:/x,mode='a,b'").options.rest,
        vec![opt("mode", Some("a,b"))]
    );
    assert_eq!(
        parse("FILE:/x,cert=a:b").options.rest,
        vec![opt("cert", Some("a:b"))]
    );
    assert_eq!(
        parse("EXEC:sh -c (a,b)").target,
        Target::Exec("sh -c (a,b)".into())
    );
}

// #57 — README §"Control plane": `,mux`, its alias `,fork`, max-children
#[test]
fn mux_and_cap() {
    let none = parse("TCP-LISTEN:8080").options;
    assert!(!none.mux);
    assert_eq!(none.cap, None);
    assert!(parse("TCP-LISTEN:8080,mux").options.mux);
    assert!(parse("TCP-LISTEN:8080,fork").options.mux);
    assert!(parse("UDP-LISTEN:53,FORK").options.mux);
    assert!(parse("UNIX-LISTEN:/s,mux").options.mux);
    assert!(parse("VSOCK-LISTEN:9,mux").options.mux);
    assert_eq!(
        parse("TCP-LISTEN:80,fork,max-children=4").options.cap,
        NonZeroU32::new(4)
    );
    assert_eq!(parse("TCP-LISTEN:80").mode(), Some(Mode::Listen));
    assert_eq!(parse("TCP:h:80").mode(), Some(Mode::Connect));
    assert_eq!(parse("EXEC:x").mode(), None);
    assert_eq!(parse("PTY").mode(), None);
}

// #57 — precise errors, byte offset into the address
#[test]
fn errors_carry_positions() {
    let cases: &[(&str, usize, Kind)] = &[
        ("", 0, Kind::Empty),
        ("BOGUS:1", 0, Kind::Keyword("BOGUS".into())),
        ("TCP4:h:1", 0, Kind::Keyword("TCP4".into())),
        ("TCP-X:h:1", 0, Kind::Keyword("TCP-X".into())),
        ("WS:h:1", 0, Kind::Keyword("WS".into())),
        ("UDP-SENDTO:h:1", 0, Kind::Keyword("UDP-SENDTO".into())),
        ("TCP:host", 8, Kind::Missing(Part::Port)),
        ("TCP", 3, Kind::Missing(Part::Host)),
        ("TCP-LISTEN", 10, Kind::Missing(Part::Port)),
        ("TCP-LISTEN:h:", 13, Kind::Missing(Part::Port)),
        ("TCP::1", 4, Kind::Missing(Part::Host)),
        ("TCP:host:99999", 9, Kind::Port("99999".into())),
        ("TCP:host:-1", 9, Kind::Port("-1".into())),
        ("TCP:host:http", 9, Kind::Port("http".into())),
        ("TCP:host:1:2", 11, Kind::Extra("2".into())),
        ("TCP:[::1]x:1", 4, Kind::Host("[::1]x".into())),
        ("TCP:[::g]:1", 4, Kind::Host("[::g]".into())),
        (
            "TCP:[fe80::1%eth0]:1",
            4,
            Kind::Host("[fe80::1%eth0]".into()),
        ),
        ("TCP:'a b':1", 4, Kind::Host("a b".into())),
        ("TCP:a/b:1", 4, Kind::Host("a/b".into())),
        ("VSOCK-CONNECT:x:1", 14, Kind::Cid("x".into())),
        ("VSOCK-CONNECT:3", 15, Kind::Missing(Part::Port)),
        ("VSOCK-CONNECT", 13, Kind::Missing(Part::Cid)),
        ("VSOCK-LISTEN:1:2", 15, Kind::Extra("2".into())),
        ("EXEC:'abc", 5, Kind::Quote),
        ("EXEC:\"abc", 5, Kind::Quote),
        ("EXEC:abc\\", 8, Kind::Escape),
        ("EXEC", 4, Kind::Missing(Part::Command)),
        ("EXEC:", 5, Kind::Missing(Part::Command)),
        ("FILE:", 5, Kind::Missing(Part::Path)),
        ("UNIX-CONNECT", 12, Kind::Missing(Part::Path)),
        ("TCP:[::1:80", 4, Kind::Bracket),
        ("EXEC:a]b", 6, Kind::Bracket),
        ("EXEC:(a]", 7, Kind::Bracket),
        ("STDIO:x", 6, Kind::Extra("x".into())),
        ("PTY:x", 4, Kind::Extra("x".into())),
        ("TCP-LISTEN:80,=x", 14, Kind::Option("=x".into())),
        ("TCP-LISTEN:80,,raw", 14, Kind::Option("".into())),
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
        (
            "TCP-LISTEN:80,mux,max-children=0",
            18,
            Kind::Value("max-children".into()),
        ),
        ("TCP-LISTEN:80,mux,fork", 18, Kind::Duplicate("mux".into())),
        (
            "TCP-LISTEN:80,mux,max-children=1,max-children=2",
            33,
            Kind::Duplicate("max-children".into()),
        ),
        ("TCP-LISTEN:80,max-children=2", 14, Kind::Cap),
    ];
    for (s, at, kind) in cases {
        assert_eq!(fail(s), Error::new(*at, kind.clone()), "{s:?}");
    }
}

// #57
#[test]
fn error_display_names_the_position() {
    assert_eq!(
        fail("TCP:host:99999").to_string(),
        "at byte 9: invalid port \"99999\""
    );
    assert_eq!(fail("TCP:host").to_string(), "at byte 8: missing port");
}

fn host() -> impl Strategy<Value = Host> {
    prop_oneof![
        "[a-z0-9][a-z0-9._-]{0,15}"
            .prop_filter("an IPv4 literal parses as V4", |n| n
                .parse::<Ipv4Addr>()
                .is_err())
            .prop_map(Host::Name),
        any::<u32>().prop_map(|v| Host::V4(Ipv4Addr::from(v))),
        any::<u128>().prop_map(|v| Host::V6(Ipv6Addr::from(v))),
    ]
}

fn text() -> impl Strategy<Value = String> {
    prop_oneof!["[ -~]{1,24}", "\\PC{1,12}"]
}

fn mode() -> impl Strategy<Value = Mode> {
    prop_oneof![Just(Mode::Connect), Just(Mode::Listen)]
}

fn net() -> impl Strategy<Value = Net> {
    prop_oneof![
        Just(Net::Tcp),
        Just(Net::Udp),
        Just(Net::Ws),
        Just(Net::Wss),
        Just(Net::Webtransport),
    ]
}

fn inet() -> impl Strategy<Value = Inet> {
    prop_oneof![
        (host(), any::<u16>())
            .prop_map(|(host, port)| Inet::Connect { host, port }),
        (proptest::option::of(host()), any::<u16>())
            .prop_map(|(host, port)| Inet::Listen { host, port }),
    ]
}

fn target() -> impl Strategy<Value = Target> {
    prop_oneof![
        text().prop_map(Target::File),
        (mode(), text()).prop_map(|(mode, path)| Target::Unix { mode, path }),
        (any::<u32>(), any::<u32>()).prop_map(|(cid, port)| {
            Target::Vsock(Vsock::Connect { cid, port })
        }),
        any::<u32>().prop_map(|port| Target::Vsock(Vsock::Listen { port })),
        (net(), inet()).prop_map(|(net, inet)| Target::Net { net, inet }),
        text().prop_map(Target::Exec),
        Just(Target::Stdio),
        Just(Target::Pty),
    ]
}

fn rest() -> impl Strategy<Value = Vec<Opt>> {
    let one = ("[a-z][a-z0-9_-]{0,10}", proptest::option::of(text()))
        .prop_filter("reserved", |(n, _)| {
            !matches!(n.as_str(), "mux" | "fork" | "max-children")
        })
        .prop_map(|(name, value)| Opt { name, value });
    prop::collection::vec(one, 0..4)
}

fn address() -> impl Strategy<Value = Address> {
    (target(), rest(), any::<bool>(), any::<Option<NonZeroU32>>()).prop_map(
        |(target, rest, mux, cap)| {
            let listen = target.mode() == Some(Mode::Listen);
            let mux = listen && (mux || cap.is_some());
            Address {
                options: Options {
                    mux,
                    cap: cap.filter(|_| mux),
                    rest,
                },
                target,
            }
        },
    )
}

fn synonyms(t: &Target) -> Vec<String> {
    let modal = |root: &str, mode: Mode, bare: bool| {
        let mut all: Vec<String> = match mode {
            Mode::Connect => {
                vec![format!("{root}-CONNECT"), format!("{root}-C")]
            }
            Mode::Listen => vec![format!("{root}-LISTEN"), format!("{root}-L")],
        };
        if bare && mode == Mode::Connect {
            all.push(root.into());
        }
        all
    };
    match t {
        Target::File(_) => vec!["FILE".into(), "OPEN".into(), "GOPEN".into()],
        Target::Unix { mode, .. } => modal("UNIX", *mode, false),
        Target::Vsock(_) => {
            modal("VSOCK", t.mode().expect("vsock has a mode"), false)
        }
        Target::Net { net, .. } => {
            let (root, bare) = match net {
                Net::Tcp => ("TCP", true),
                Net::Udp => ("UDP", true),
                Net::Ws => ("WS", false),
                Net::Wss => ("WSS", false),
                Net::Webtransport => ("WEBTRANSPORT", false),
            };
            modal(root, t.mode().expect("net targets have a mode"), bare)
        }
        Target::Exec(_) => vec!["EXEC".into()],
        Target::Stdio => vec!["STDIO".into(), "-".into()],
        Target::Pty => vec!["PTY".into()],
    }
}

fn scramble(s: &str, mask: u64) -> String {
    s.chars()
        .enumerate()
        .map(|(i, c)| {
            if mask >> (i % 64) & 1 == 1 {
                c.to_ascii_lowercase()
            } else {
                c
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
struct Spelling {
    synonym: usize,
    mask: u64,
    fork: bool,
    swap: bool,
}

fn spelling() -> impl Strategy<Value = Spelling> {
    (any::<usize>(), any::<u64>(), any::<bool>(), any::<bool>()).prop_map(
        |(synonym, mask, fork, swap)| Spelling {
            synonym,
            mask,
            fork,
            swap,
        },
    )
}

fn respell(a: &Address, sp: &Spelling) -> String {
    let bare = Address {
        target: a.target.clone(),
        options: Options::default(),
    }
    .to_string();
    let split = bare.find([':', ',']).unwrap_or(bare.len());
    let names = synonyms(&a.target);
    let keyword = scramble(&names[sp.synonym % names.len()], sp.mask);
    let mut out = format!("{keyword}{}", &bare[split..]);
    for o in &a.options.rest {
        let one = Address {
            target: Target::Stdio,
            options: Options {
                rest: vec![o.clone()],
                ..Options::default()
            },
        }
        .to_string();
        let o = &one["STDIO,".len()..];
        let (n, v) = o.split_at(o.find('=').unwrap_or(o.len()));
        out.push(',');
        out.push_str(&scramble(&n.to_uppercase(), sp.mask.rotate_left(7)));
        out.push_str(v);
    }
    let mux = a
        .options
        .mux
        .then(|| scramble(if sp.fork { ",FORK" } else { ",MUX" }, sp.mask));
    let cap = a.options.cap.map(|c| {
        format!("{}={c}", scramble(",MAX-CHILDREN", sp.mask.rotate_left(3)))
    });
    let (first, second) = if sp.swap { (cap, mux) } else { (mux, cap) };
    out.extend(first);
    out.extend(second);
    out
}

proptest! {
    // #57 — parse ∘ print is the identity on typed addresses
    #[test]
    fn roundtrip(a in address()) {
        let s = a.to_string();
        prop_assert_eq!(Address::parse(&s), Ok(a), "{}", s);
    }

    // #57 — print(parse(s)) is canonical for every spelling of an address
    #[test]
    fn canonical(a in address(), sp in spelling()) {
        let s = respell(&a, &sp);
        let parsed = Address::parse(&s);
        prop_assert_eq!(parsed.as_ref().map(ToString::to_string), Ok(a.to_string()), "{}", s);
        prop_assert_eq!(parsed, Ok(a), "{}", s);
    }

    // #57 — arbitrary input never panics, and anything accepted re-prints
    #[test]
    fn arbitrary_input(s in "[ -~]{0,40}") {
        if let Ok(a) = Address::parse(&s) {
            let p = a.to_string();
            prop_assert_eq!(Address::parse(&p), Ok(a), "{} → {}", s, p);
        }
    }

    // #57 — an out-of-range port is rejected at the port's offset
    #[test]
    fn port_out_of_range(h in "[a-z]{1,8}", p in 65536u64..) {
        let s = format!("TCP:{h}:{p}");
        prop_assert_eq!(
            Address::parse(&s),
            Err(Error::new(5 + h.len(), Kind::Port(p.to_string())))
        );
    }

    // #57 — an unknown address type is rejected at byte 0
    #[test]
    fn unknown_keyword(k in "[A-Z][A-Z0-9-]{0,10}") {
        prop_assume!(!synonyms_all().contains(&k));
        prop_assert_eq!(
            Address::parse(&format!("{k}:x")),
            Err(Error::new(0, Kind::Keyword(k.clone())))
        );
    }

    // #57 — a host with a character outside the hostname set is rejected at the host
    #[test]
    fn invalid_host(a in "[a-z]{0,4}", c in "[@#$%^&*+~/]", b in "[a-z]{0,4}") {
        let h = format!("{a}{c}{b}");
        prop_assert_eq!(
            Address::parse(&format!("WS-CONNECT:{h}:1")),
            Err(Error::new(11, Kind::Host(h.clone())))
        );
    }

    // #57 — an unterminated quote is rejected at the quote's offset
    #[test]
    fn unterminated_quote(a in address(), body in "[^']{0,8}") {
        let s = format!("{a},x='{body}");
        let at = s.len() - body.len() - 1;
        prop_assert_eq!(Address::parse(&s), Err(Error::new(at, Kind::Quote)));
    }

    // #57 — an unclosed bracket is rejected at the bracket's offset
    #[test]
    fn unclosed_bracket(a in address(), open in "[\\[({]", body in "[a-z]{0,8}") {
        let s = format!("{a},x={open}{body}");
        let at = s.len() - body.len() - 1;
        prop_assert_eq!(Address::parse(&s), Err(Error::new(at, Kind::Bracket)));
    }
}

fn synonyms_all() -> Vec<String> {
    let mut all = vec![
        "FILE", "OPEN", "GOPEN", "EXEC", "STDIO", "PTY", "TCP", "UDP",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    for root in ["TCP", "UDP", "WS", "WSS", "WEBTRANSPORT", "UNIX", "VSOCK"] {
        for suffix in ["CONNECT", "C", "LISTEN", "L"] {
            all.push(format!("{root}-{suffix}"));
        }
    }
    all
}
