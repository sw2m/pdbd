use crate::fixture::{name, num, parse, same, tcp};
use crate::{Address, Error, Host, Kind, Zone};
use proptest::prelude::*;
use std::net::Ipv6Addr;

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

proptest! {
    // #57
    #[test]
    fn invalid_host(a in "[a-z]{0,4}", c in "[@#$%^&*+~/!=?]", b in "[a-z]{0,4}") {
        let h = format!("{a}{c}{b}");
        prop_assert_eq!(
            Address::parse(&format!("WS-CONNECT:{h}:1")),
            Err(Error::new(11, Kind::Host(h.clone())))
        );
    }
}
