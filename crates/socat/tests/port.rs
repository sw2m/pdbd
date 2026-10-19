mod support;

use proptest::prelude::*;
use socat::{Address, Error, Kind};
use support::fixture::{name, parse, same, svc, tcp};

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

proptest! {
    #![proptest_config(support::strategy::config())]

    // #57
    #[test]
    fn overflow(h in "[a-z]{1,8}", p in 65536u64..) {
        let s = format!("TCP:{h}:{p}");
        prop_assert_eq!(
            Address::parse(&s),
            Err(Error::new(5 + h.len(), Kind::Port(p.to_string())))
        );
    }
}
