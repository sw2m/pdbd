use super::*;
use crate::lex::Quoted;
use crate::{
    Host, Inet, Kind, Name, Net, Opt, Part, Port, Resource, Service, Text,
    Unchecked, Vsock, Web, Zone,
};
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

fn text(s: &str) -> Text {
    Text::new(s).expect("non-empty text")
}

fn name(n: &str) -> Host {
    Host::Name(Name::new(n).expect("valid host name"))
}

fn num(p: u16) -> Port {
    Port::Number(p)
}

fn svc(s: &str) -> Port {
    Port::Service(Service::new(s).expect("valid service"))
}

fn opt(n: &str, v: Option<&str>) -> Opt {
    Opt::new(n, v).expect("valid option")
}

fn res(r: &str) -> Resource {
    Resource::new(r).expect("valid resource")
}

fn address(target: Target, mux: bool, cap: u32, rest: Vec<Opt>) -> Address {
    let options = Options::new(mux, NonZeroU32::new(cap), Unchecked::new(rest))
        .expect("valid options");
    Address::new(target, options).expect("valid address")
}

fn plain(target: Target, rest: Vec<Opt>) -> Address {
    address(target, false, 0, rest)
}

fn tcp(host: Host, port: Port) -> Target {
    Target::Net {
        net: Net::Tcp,
        inet: Inet::Connect { host, port },
    }
}

fn web(web: Web, inet: Inet, resource: &str) -> Target {
    Target::Web {
        web,
        inet,
        resource: res(resource),
    }
}

fn exec(s: &str) -> Target {
    parse(s).target().clone()
}

fn table(cases: &[(&str, usize, Kind)]) {
    for (s, at, kind) in cases {
        assert_eq!(fail(s), Error::new(*at, kind.clone()), "{s:?}");
    }
}

fn same(cases: &[(&str, &str)]) {
    for (s, canonical) in cases {
        assert_eq!(parse(s), parse(canonical), "{s}");
        assert_eq!(parse(s).to_string(), *canonical, "{s}");
    }
}

// #57
#[test]
fn readme_transports() {
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
fn ipv6() {
    assert_eq!(
        parse("TCP:[::1]:80").target(),
        &tcp(Host::V6(Ipv6Addr::LOCALHOST, None), num(80))
    );
    let zone = Zone::new("lo").expect("zone");
    assert_eq!(
        parse("TCP:[fe80::1%lo]:80").target(),
        &tcp(
            Host::V6("fe80::1".parse().expect("ip"), Some(zone)),
            num(80)
        )
    );
    same(&[
        ("TCP:[0:0:0::1]:80", "TCP-CONNECT:[::1]:80"),
        ("TCP-LISTEN:[::]:80", "TCP-LISTEN:[::]:80"),
        ("TCP:[fe80::1%eth0]:80", "TCP-CONNECT:[fe80::1%eth0]:80"),
    ]);
}

// #57
#[test]
fn names() {
    assert_eq!(
        parse("TCP:bücher.de:80").target(),
        &tcp(name("bücher.de"), num(80))
    );
    same(&[
        ("TCP:BÜCHER.DE:1", "TCP-CONNECT:bÜcher.de:1"),
        ("TCP:a_b-c.d:1", "TCP-CONNECT:a_b-c.d:1"),
    ]);
}

// #57
#[test]
fn ports() {
    assert_eq!(parse("TCP:h:http").target(), &tcp(name("h"), svc("http")));
    assert_eq!(parse("TCP:h:0x50").target(), &tcp(name("h"), svc("0x50")));
    same(&[
        ("TCP:h:+80", "TCP-CONNECT:h:80"),
        ("TCP:h:080", "TCP-CONNECT:h:80"),
        ("TCP:h:0", "TCP-CONNECT:h:0"),
        ("TCP:h:65535", "TCP-CONNECT:h:65535"),
        ("UDP-LISTEN:domain", "UDP-LISTEN:domain"),
        ("TCP:h:http", "TCP-CONNECT:h:http"),
    ]);
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
fn resources() {
    let h = || name("host");
    assert_eq!(
        exec("WS-CONNECT:host:443/socket"),
        web(
            Web::Ws,
            Inet::Connect {
                host: h(),
                port: num(443)
            },
            "/socket"
        )
    );
    assert_eq!(
        exec("WSS-LISTEN:0.0.0.0:443/chat"),
        web(
            Web::Wss,
            Inet::Listen {
                host: Some(Host::V4(Ipv4Addr::UNSPECIFIED)),
                port: num(443)
            },
            "/chat"
        )
    );
    assert_eq!(
        exec("WS-LISTEN:8080/x"),
        web(
            Web::Ws,
            Inet::Listen {
                host: None,
                port: num(8080)
            },
            "/x"
        )
    );
    assert_eq!(
        exec("ws://host/socket?a=1"),
        web(
            Web::Ws,
            Inet::Connect {
                host: h(),
                port: num(80)
            },
            "/socket?a=1"
        )
    );
    same(&[
        ("WS-CONNECT:h:80/", "WS-CONNECT:h:80"),
        ("WS-CONNECT:h:80/a?t=1:2", "WS-CONNECT:h:80'/a?t=1:2'"),
        ("ws://h", "WS-CONNECT:h:80"),
        ("ws://h/", "WS-CONNECT:h:80"),
        ("WS://h:9/x", "WS-CONNECT:h:9/x"),
        ("wss://h", "WSS-CONNECT:h:443"),
        ("WSS://h:8443/a?b=1", "WSS-CONNECT:h:8443/a?b=1"),
        ("https://h/wt", "WEBTRANSPORT-CONNECT:h:443/wt"),
        ("ws://[::1]:9/x", "WS-CONNECT:[::1]:9/x"),
        ("ws://h/a:b", "WS-CONNECT:h:80'/a:b'"),
    ]);
}

// #57
#[test]
fn quoting() {
    assert_eq!(
        exec(r#"EXEC:"say \"hi\", ok""#),
        Target::Exec(text(r#"say "hi", ok"#))
    );
    assert_eq!(exec(r"EXEC:a\,b"), Target::Exec(text("a,b")));
    assert_eq!(exec(r"EXEC:'it'\''s'"), Target::Exec(text("it's")));
    assert_eq!(parse("EXEC:it\\'s").to_string(), r"EXEC:'it'\''s'");
    assert_eq!(exec("EXEC:sh -c (a,b)"), Target::Exec(text("sh -c (a,b)")));
    assert_eq!(
        parse("FILE:/x,mode='a,b'").options().rest().as_slice(),
        &[opt("mode", Some("a,b"))]
    );
    assert_eq!(
        parse("FILE:/x,cert=a:b").options().rest().as_slice(),
        &[opt("cert", Some("a:b"))]
    );
}

// #57 — socat(1) named escapes; no \xHH
#[test]
fn escapes() {
    assert_eq!(exec(r"EXEC:echo\ a\nb"), Target::Exec(text("echo a\nb")));
    assert_eq!(
        exec(r"EXEC:\r\t\f\b\a"),
        Target::Exec(text("\r\t\u{c}\u{8}\u{7}"))
    );
    assert_eq!(exec(r#"EXEC:"a\nb""#), Target::Exec(text("a\nb")));
    assert_eq!(exec(r"EXEC:'a\nb'"), Target::Exec(text(r"a\nb")));
    assert_eq!(exec(r"EXEC:\q"), Target::Exec(text("q")));
    assert_eq!(exec(r"EXEC:\x41"), Target::Exec(text("x41")));
    assert_eq!(
        parse(r"FILE:/x,v=a\rb").options().rest().as_slice(),
        &[opt("v", Some("a\rb"))]
    );
    assert_eq!(parse(r"EXEC:a\nb").to_string(), "EXEC:'a\nb'");
}

// #57
#[test]
fn brackets() {
    assert_eq!(exec("EXEC:a]b"), Target::Exec(text("a]b")));
    assert_eq!(exec("EXEC:a)b}"), Target::Exec(text("a)b}")));
    assert_eq!(exec("EXEC:(a)b]"), Target::Exec(text("(a)b]")));
    assert_eq!(parse("EXEC:a]b").to_string(), "EXEC:'a]b'");
}

// #57
#[test]
fn dual() {
    table(&[
        ("FILE:/tmp/a!!TCP:host:80", 11, Kind::Dual),
        ("STDIO!!STDIO", 5, Kind::Dual),
        ("TCP-LISTEN:80,cert=a!!b", 20, Kind::Dual),
        ("EXEC:é!!x", 7, Kind::Dual),
    ]);
    assert_eq!(exec("EXEC:'a!!b'"), Target::Exec(text("a!!b")));
    assert_eq!(exec(r"EXEC:a\!!b"), Target::Exec(text("a!!b")));
    assert_eq!(exec("EXEC:(a!!b)"), Target::Exec(text("(a!!b)")));
    assert_eq!(exec("EXEC:a!b"), Target::Exec(text("a!b")));
    assert_eq!(parse("EXEC:'a!!b'").to_string(), "EXEC:'a!!b'");
    assert_eq!(parse("EXEC:a!b").to_string(), "EXEC:a!b");
    let printed = [
        (plain(Target::Gopen(text("/a!!b")), vec![]), "GOPEN:'/a!!b'"),
        (
            plain(Target::Stdio, vec![opt("cert", Some("x!!"))]),
            "STDIO,cert='x!!'",
        ),
        (
            plain(
                Target::Unix {
                    mode: Mode::Connect,
                    path: text("!!"),
                },
                vec![],
            ),
            "UNIX-CONNECT:'!!'",
        ),
    ];
    for (a, want) in printed {
        assert_eq!(a.to_string(), want);
        assert_eq!(parse(want), a);
    }
}

// #57 — README §"Control plane": `,mux`, its alias `,fork`, max-children
#[test]
fn mux_and_cap() {
    let none = parse("TCP-LISTEN:8080");
    assert!(!none.options().mux());
    assert_eq!(none.options().cap(), None);
    assert!(parse("TCP-LISTEN:8080,mux").options().mux());
    assert!(parse("TCP-LISTEN:8080,fork").options().mux());
    assert!(parse("UDP-LISTEN:53,FORK").options().mux());
    assert!(parse("UNIX-LISTEN:/s,mux").options().mux());
    assert!(parse("VSOCK-LISTEN:9,mux").options().mux());
    assert!(parse("WS-LISTEN:9/x,mux").options().mux());
    assert_eq!(
        parse("TCP-LISTEN:80,fork,max-children=4").options().cap(),
        NonZeroU32::new(4)
    );
    assert_eq!(parse("TCP-LISTEN:80").mode(), Some(Mode::Listen));
    assert_eq!(parse("TCP:h:80").mode(), Some(Mode::Connect));
    assert_eq!(parse("EXEC:x").mode(), None);
    assert_eq!(parse("PTY").mode(), None);
    assert_eq!(parse("GOPEN:/x").mode(), None);
}

// #57
#[test]
fn near_miss_options() {
    same(&[
        (
            "TCP-LISTEN:80,max_children=4,mux",
            "TCP-LISTEN:80,mux,max-children=4",
        ),
        (
            "TCP-LISTEN:80,maxchildren=4,mux",
            "TCP-LISTEN:80,mux,max-children=4",
        ),
        (
            "TCP-LISTEN:80,MaxChildren=4,mux",
            "TCP-LISTEN:80,mux,max-children=4",
        ),
        (
            "TCP-LISTEN:80,max__children=4,MU_X",
            "TCP-LISTEN:80,mux,max-children=4",
        ),
        ("TCP-LISTEN:80,Fo-Rk", "TCP-LISTEN:80,mux"),
    ]);
    table(&[
        ("TCP-LISTEN:80,max_children=4", 14, Kind::Cap),
        (
            "TCP:h:1,max_children=2",
            8,
            Kind::Option("max_children".into()),
        ),
        ("TCP:h:1,MU_X", 8, Kind::Option("mu_x".into())),
        (
            "TCP-LISTEN:80,mux,max-children=1,maxchildren=2",
            33,
            Kind::Duplicate("max-children".into()),
        ),
        ("TCP-LISTEN:80,fork=1", 14, Kind::Value("fork".into())),
    ]);
}

// #57
#[test]
fn empty_options() {
    same(&[
        ("TCP-LISTEN:80,", "TCP-LISTEN:80"),
        ("TCP-LISTEN:80,,raw,", "TCP-LISTEN:80,raw"),
        ("STDIO,,,", "STDIO"),
        ("STDIO,x=", "STDIO,x=''"),
    ]);
    assert_eq!(
        parse("STDIO,x=,y").options().rest().as_slice(),
        &[opt("x", Some("")), opt("y", None)]
    );
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

// #57
#[test]
fn errors_carry_positions() {
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
        ("WS-CONNECT:h:80/a b", 15, Kind::Resource("/a b".into())),
        ("ws://", 5, Kind::Missing(Part::Host)),
        ("ws://h:99999/", 7, Kind::Port("99999".into())),
        ("ws:x", 0, Kind::Keyword("ws".into())),
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
    ]);
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
fn non_ascii_offsets() {
    table(&[
        ("EXEC:é\\", 7, Kind::Escape),
        ("EXEC:(é]", 8, Kind::Bracket),
        ("EXEC:é[x", 7, Kind::Bracket),
        ("EXEC:é'x", 7, Kind::Quote),
        ("TCP:h:é", 6, Kind::Port("é".into())),
        ("TCP:é:1:é", 9, Kind::Extra("é".into())),
        ("STDIO,é", 6, Kind::Option("é".into())),
        ("FILE:é,x=é,mux", 13, Kind::Option("mux".into())),
        ("VSOCK-LISTEN:é", 13, Kind::Port("é".into())),
        ("TCP:'é b':1", 4, Kind::Host("é b".into())),
    ]);
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
    assert_eq!(Name::new("Bü").expect("name").as_str(), "bü");
    for bad in ["", "a%b", "a]b", "a b"] {
        assert_eq!(Zone::new(bad), None, "{bad:?}");
    }
    for bad in ["", "80", "+x", "-x", "a b", "é"] {
        assert_eq!(Service::new(bad), None, "{bad:?}");
    }
    assert!(Service::new("http").is_some());
    for bad in ["", "x", "/a b", "/a\nb"] {
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

// #57
#[test]
fn error_display_names_the_position() {
    assert_eq!(
        fail("TCP:host:99999").to_string(),
        "at byte 9: invalid port \"99999\""
    );
    assert_eq!(fail("TCP:host").to_string(), "at byte 8: missing port");
    assert_eq!(
        fail("STDIO!!STDIO").to_string(),
        "at byte 5: dual address \"!!\" is not supported"
    );
}

fn host() -> impl Strategy<Value = Host> {
    let zone = proptest::option::of("[a-z0-9]{1,6}")
        .prop_map(|z| z.map(|z| Zone::new(&z).expect("zone")));
    prop_oneof![
        prop_oneof!["[a-z0-9][a-z0-9._-]{0,15}", "[a-zé-ü][a-zé-ü.]{0,7}"]
            .prop_filter_map("not a name", |n| Name::new(&n))
            .prop_map(Host::Name),
        any::<u32>().prop_map(|v| Host::V4(Ipv4Addr::from(v))),
        (any::<u128>(), zone).prop_map(|(v, z)| Host::V6(Ipv6Addr::from(v), z)),
    ]
}

fn port() -> impl Strategy<Value = Port> {
    prop_oneof![
        any::<u16>().prop_map(Port::Number),
        "[a-z][a-z0-9-]{0,8}"
            .prop_filter_map("not a service", |s| Service::new(&s))
            .prop_map(Port::Service),
    ]
}

fn strings() -> impl Strategy<Value = String> {
    prop_oneof![
        "[ -~]{1,24}",
        "\\PC{1,12}",
        "[\\x00-\\x1f a-z!]{1,8}",
        "[a-z]{0,3}!![a-z]{0,3}",
    ]
}

fn texts() -> impl Strategy<Value = Text> {
    strings().prop_filter_map("empty", Text::new)
}

fn paths() -> impl Strategy<Value = Text> {
    prop_oneof![
        texts(),
        strings().prop_filter_map("empty", |s| Text::new(format!("/{s}"))),
    ]
}

fn values() -> impl Strategy<Value = Option<String>> {
    proptest::option::of(prop_oneof![
        Just(String::new()),
        "[ -~]{0,10}",
        "\\PC{0,8}",
        "[\\x00-\\x1f]{1,4}",
    ])
}

fn mode() -> impl Strategy<Value = Mode> {
    prop_oneof![Just(Mode::Connect), Just(Mode::Listen)]
}

fn inet() -> impl Strategy<Value = Inet> {
    prop_oneof![
        (host(), port()).prop_map(|(host, port)| Inet::Connect { host, port }),
        (proptest::option::of(host()), port())
            .prop_map(|(host, port)| Inet::Listen { host, port }),
    ]
}

fn resource() -> impl Strategy<Value = Resource> {
    prop_oneof![
        Just(Resource::default()),
        "/[!-~]{0,12}".prop_filter_map("invalid", |r| Resource::new(&r)),
    ]
}

fn target() -> impl Strategy<Value = Target> {
    let net = prop_oneof![Just(Net::Tcp), Just(Net::Udp)];
    let web =
        prop_oneof![Just(Web::Ws), Just(Web::Wss), Just(Web::Webtransport)];
    prop_oneof![
        texts().prop_map(Target::File),
        texts().prop_map(Target::Open),
        paths().prop_map(Target::Gopen),
        (mode(), texts()).prop_map(|(mode, path)| Target::Unix { mode, path }),
        (mode(), texts())
            .prop_map(|(mode, name)| Target::Abstract { mode, name }),
        (any::<u32>(), any::<u32>()).prop_map(|(cid, port)| {
            Target::Vsock(Vsock::Connect { cid, port })
        }),
        any::<u32>().prop_map(|port| Target::Vsock(Vsock::Listen { port })),
        (net, inet()).prop_map(|(net, inet)| Target::Net { net, inet }),
        (web, inet(), resource()).prop_map(|(web, inet, resource)| {
            Target::Web {
                web,
                inet,
                resource,
            }
        }),
        texts().prop_map(Target::Exec),
        Just(Target::Stdio),
        Just(Target::Pty),
    ]
}

fn rest() -> impl Strategy<Value = Vec<Opt>> {
    let one = ("[a-z][a-z0-9_-]{0,10}", values())
        .prop_filter_map("reserved", |(n, v)| Opt::new(&n, v.as_deref()));
    prop::collection::vec(one, 0..4)
}

fn addresses() -> impl Strategy<Value = Address> {
    (target(), rest(), any::<bool>(), any::<Option<NonZeroU32>>()).prop_map(
        |(target, rest, mux, cap)| {
            let listen = target.mode() == Some(Mode::Listen);
            let mux = listen && (mux || cap.is_some());
            let options =
                Options::new(mux, cap.filter(|_| mux), Unchecked::new(rest))
                    .expect("valid options");
            Address::new(target, options).expect("valid address")
        },
    )
}

fn modal(root: &str, mode: Mode, bare: &[&str]) -> Vec<String> {
    let mut all: Vec<String> = match mode {
        Mode::Connect => vec![
            format!("{root}-CONNECT"),
            format!("{root}-C"),
            format!("C-{root}"),
            format!("CONNECT-{root}"),
        ],
        Mode::Listen => vec![
            format!("{root}-LISTEN"),
            format!("{root}-L"),
            format!("L-{root}"),
            format!("LISTEN-{root}"),
        ],
    };
    if mode == Mode::Connect {
        all.extend(bare.iter().map(|b| b.to_string()));
    }
    all
}

fn synonyms(t: &Target) -> Vec<String> {
    let modal = |root, bare| modal(root, t.mode().expect("modal"), bare);
    match t {
        Target::File(_) => vec!["FILE".into()],
        Target::Open(_) => vec!["OPEN".into()],
        Target::Gopen(p) if p.as_str().contains('/') => {
            vec!["GOPEN".into(), String::new()]
        }
        Target::Gopen(_) => vec!["GOPEN".into()],
        Target::Unix { .. } => modal("UNIX", &["UNIX", "LOCAL"]),
        Target::Abstract { .. } => modal("ABSTRACT", &[]),
        Target::Vsock(_) => modal("VSOCK", &[]),
        Target::Net { net: Net::Tcp, .. } => modal("TCP", &["TCP"]),
        Target::Net { net: Net::Udp, .. } => modal("UDP", &["UDP"]),
        Target::Web { web: Web::Ws, .. } => modal("WS", &[]),
        Target::Web { web: Web::Wss, .. } => modal("WSS", &[]),
        Target::Web {
            web: Web::Webtransport,
            ..
        } => modal("WEBTRANSPORT", &[]),
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
    cap: usize,
    url: bool,
    empty: bool,
}

fn spelling() -> impl Strategy<Value = Spelling> {
    (
        any::<usize>(),
        any::<u64>(),
        any::<bool>(),
        any::<bool>(),
        any::<usize>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(synonym, mask, fork, swap, cap, url, empty)| {
            Spelling {
                synonym,
                mask,
                fork,
                swap,
                cap,
                url,
                empty,
            }
        })
}

fn url(t: &Target) -> Option<String> {
    let Target::Web {
        web,
        inet: Inet::Connect { host, port },
        resource,
    } = t
    else {
        return None;
    };
    let scheme = match web {
        Web::Ws => "ws",
        Web::Wss => "wss",
        Web::Webtransport => "https",
    };
    let port = match port {
        Port::Number(n) => n.to_string(),
        Port::Service(s) => s.as_str().to_string(),
    };
    Some(format!(
        "{scheme}://{host}:{port}{}",
        Quoted(resource.as_str())
    ))
}

fn respell(a: &Address, sp: &Spelling) -> String {
    let bare = Address::new(a.target().clone(), Options::default())
        .expect("address")
        .to_string();
    let split = bare.find([':', ',']).unwrap_or(bare.len());
    let names = synonyms(a.target());
    let keyword = scramble(&names[sp.synonym % names.len()], sp.mask);
    let mut out = match url(a.target()) {
        Some(u) if sp.url => u,
        _ if keyword.is_empty() => bare[split + 1..].to_string(),
        _ => format!("{keyword}{}", &bare[split..]),
    };
    for o in a.options().rest().as_slice() {
        let one = plain(Target::Stdio, vec![o.clone()]).to_string();
        let o = &one["STDIO,".len()..];
        let (n, v) = o.split_at(o.find('=').unwrap_or(o.len()));
        out.push(',');
        out.push_str(&scramble(&n.to_uppercase(), sp.mask.rotate_left(7)));
        out.push_str(v);
        if sp.empty {
            out.push(',');
        }
    }
    let mux = a
        .options()
        .mux()
        .then(|| scramble(if sp.fork { ",FORK" } else { ",MU_X" }, sp.mask));
    let caps = ["MAX-CHILDREN", "max_children", "MaxChildren", "maxchildren"];
    let cap = a.options().cap().map(|c| {
        let spelled =
            scramble(caps[sp.cap % caps.len()], sp.mask.rotate_left(3));
        format!(",{spelled}={c}")
    });
    let (first, second) = if sp.swap { (cap, mux) } else { (mux, cap) };
    out.extend(first);
    out.extend(second);
    if sp.empty {
        out.push(',');
    }
    out
}

fn check(s: &str) -> Result<(), TestCaseError> {
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
enum Edit {
    Insert(usize, u8),
    Delete(usize),
    Substitute(usize, u8),
}

fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        (any::<usize>(), any::<u8>()).prop_map(|(i, b)| Edit::Insert(i, b)),
        any::<usize>().prop_map(Edit::Delete),
        (any::<usize>(), any::<u8>()).prop_map(|(i, b)| Edit::Substitute(i, b)),
    ]
}

impl Edit {
    fn apply(&self, s: &str) -> Vec<u8> {
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

fn keywords() -> Vec<String> {
    let mut all: Vec<String> = [
        "FILE", "OPEN", "GOPEN", "EXEC", "STDIO", "PTY", "TCP", "UDP", "UNIX",
        "LOCAL", "WS", "WSS", "HTTPS",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let roots = [
        "TCP",
        "UDP",
        "WS",
        "WSS",
        "WEBTRANSPORT",
        "UNIX",
        "VSOCK",
        "ABSTRACT",
    ];
    for root in roots {
        all.extend(modal(root, Mode::Connect, &[]));
        all.extend(modal(root, Mode::Listen, &[]));
    }
    all
}

proptest! {
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
    fn arbitrary_input(s in "[ -~]{0,40}") {
        check(&s)?;
    }

    // #57
    #[test]
    fn arbitrary_unicode(s in "\\PC{0,24}") {
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

    // #57
    #[test]
    fn port_out_of_range(h in "[a-z]{1,8}", p in 65536u64..) {
        let s = format!("TCP:{h}:{p}");
        prop_assert_eq!(
            Address::parse(&s),
            Err(Error::new(5 + h.len(), Kind::Port(p.to_string())))
        );
    }

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

    // #57
    #[test]
    fn invalid_host(a in "[a-z]{0,4}", c in "[@#$%^&*+~/!=?]", b in "[a-z]{0,4}") {
        let h = format!("{a}{c}{b}");
        prop_assert_eq!(
            Address::parse(&format!("WS-CONNECT:{h}:1")),
            Err(Error::new(11, Kind::Host(h.clone())))
        );
    }

    // #57
    #[test]
    fn unterminated_quote(a in addresses(), body in "[^']{0,8}") {
        let s = format!("{a},x='{body}");
        let at = s.len() - body.len() - 1;
        prop_assert_eq!(Address::parse(&s), Err(Error::new(at, Kind::Quote)));
    }

    // #57
    #[test]
    fn unclosed_bracket(a in addresses(), open in "[\\[({]", body in "[a-z]{0,8}") {
        let s = format!("{a},x={open}{body}");
        let at = s.len() - body.len() - 1;
        prop_assert_eq!(Address::parse(&s), Err(Error::new(at, Kind::Bracket)));
    }

    // #57
    #[test]
    fn dual_rejected(a in addresses(), tail in "[a-z]{0,6}") {
        let s = format!("{a},x=y!!{tail}");
        let at = s.len() - tail.len() - 2;
        prop_assert_eq!(Address::parse(&s), Err(Error::new(at, Kind::Dual)));
    }
}
