use crate::fixture::{exec, name, num, same, table, web};
use crate::{Host, Inet, Kind, Web};
use std::net::Ipv4Addr;

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
        ("ws://h?x=1", "WS-CONNECT:h:80/?x=1"),
        ("ws://h:9?x", "WS-CONNECT:h:9/?x"),
        ("WS-LISTEN:8080'/a:b'", "WS-LISTEN:8080'/a:b'"),
    ]);
}

// #57
#[test]
fn slashed_listen() {
    table(&[
        ("WS-LISTEN:'h/x':80", 10, Kind::Port("h/x".into())),
        ("WS-LISTEN:8080/a:b", 10, Kind::Port("8080/a".into())),
        ("WSS-LISTEN:h/x:1:2", 11, Kind::Port("h/x".into())),
    ]);
    assert_eq!(
        exec("WS-LISTEN:8080'/a:b'"),
        web(
            Web::Ws,
            Inet::Listen {
                host: None,
                port: num(8080)
            },
            "/a:b"
        )
    );
}
