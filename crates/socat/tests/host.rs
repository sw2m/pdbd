mod support;

use proptest::prelude::*;
use socat::{Address, Error, Host, Kind, Zone};
use std::net::Ipv6Addr;
use support::fixture::{name, num, parse, same, table, tcp};

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
    same(&[
        ("TCP:a-b.c:1", "TCP-CONNECT:a-b.c:1"),
        ("TCP:A.B:1", "TCP-CONNECT:a.b:1"),
    ]);
}

// #57: RFC 1123 §2.1
#[test]
fn labels() {
    let long = "a".repeat(64);
    let tall = [
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(61),
    ]
    .join(".");
    assert_eq!(tall.len(), 253);
    assert_eq!(
        parse(&format!("TCP:{tall}:1")).target(),
        &tcp(name(&tall), num(1))
    );
    assert_eq!(
        parse(&format!("TCP:{tall}.:1")).target(),
        &tcp(name(&format!("{tall}.")), num(1))
    );
    let over = format!("{tall}e");
    table(&[
        ("TCP:a_b:1", 4, Kind::Host("a_b".into())),
        ("TCP:-a:1", 4, Kind::Host("-a".into())),
        ("TCP:a-:1", 4, Kind::Host("a-".into())),
        ("TCP:a.-b:1", 4, Kind::Host("a.-b".into())),
        ("TCP:a..b:1", 4, Kind::Host("a..b".into())),
        ("TCP:.a:1", 4, Kind::Host(".a".into())),
        (&format!("TCP:{long}:1"), 4, Kind::Host(long.clone())),
        (&format!("TCP:{over}:1"), 4, Kind::Host(over.clone())),
    ]);
}

// #57: UTS #46 nontransitional, not strict IDNA2008 (NV8 U+2044 passes)
#[cfg(feature = "idn")]
#[test]
fn unicode() {
    assert_eq!(
        parse("TCP:bücher.de:80").target(),
        &tcp(name("bücher.de"), num(80))
    );
    same(&[
        ("TCP:BÜCHER.DE:1", "TCP-CONNECT:bücher.de:1"),
        ("TCP:xn--bcher-kva.de:1", "TCP-CONNECT:bücher.de:1"),
        ("TCP:a\u{2044}b:1", "TCP-CONNECT:a\u{2044}b:1"),
    ]);
    table(&[("TCP:a\u{ff0f}b:1", 4, Kind::Host("a\u{ff0f}b".into()))]);
}

// #57: RFC 1123 alone without the idn feature
#[cfg(not(feature = "idn"))]
#[test]
fn unicode() {
    table(&[("TCP:bücher.de:1", 4, Kind::Host("bücher.de".into()))]);
}

// #57: RFC 1123 §2.1, a root dot, and no all-numeric top label
#[test]
fn absolute() {
    same(&[
        ("TCP:a.:1", "TCP-CONNECT:a.:1"),
        ("TCP:A.B.:1", "TCP-CONNECT:a.b.:1"),
        ("TCP:1a.b:1", "TCP-CONNECT:1a.b:1"),
        ("TCP:a.b1:1", "TCP-CONNECT:a.b1:1"),
    ]);
    table(&[
        ("TCP:.:1", 4, Kind::Host(".".into())),
        ("TCP:a..:1", 4, Kind::Host("a..".into())),
        ("TCP:1:1", 4, Kind::Host("1".into())),
        ("TCP:a.1:1", 4, Kind::Host("a.1".into())),
        ("TCP:a.1.:1", 4, Kind::Host("a.1.".into())),
        ("TCP:01.2.3.4:1", 4, Kind::Host("01.2.3.4".into())),
        ("TCP:0x7f.1:1", 4, Kind::Host("0x7f.1".into())),
    ]);
}

// #57: RFC 4007 §11, RFC 9844 §6
#[test]
fn zones() {
    same(&[(
        "TCP:[fe80::1%abcdefghijklmno]:1",
        "TCP-CONNECT:[fe80::1%abcdefghijklmno]:1",
    )]);
    table(&[
        (
            "TCP:[fe80::1%abcdefghijklmnop]:1",
            4,
            Kind::Host("[fe80::1%abcdefghijklmnop]".into()),
        ),
        ("TCP:[fe80::1%~]:1", 4, Kind::Host("[fe80::1%~]".into())),
    ]);
}

proptest! {
    #![proptest_config(support::strategy::config())]

    // #57
    #[test]
    fn invalid(a in "[a-z]{0,4}", c in "[@#$%^&*+~/!=?]", b in "[a-z]{0,4}") {
        let h = format!("{a}{c}{b}");
        prop_assert_eq!(
            Address::parse(&format!("WS-CONNECT:{h}:1")),
            Err(Error::new(11, Kind::Host(h.clone())))
        );
    }
}
