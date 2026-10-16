use crate::fixture::{opt, parse, same, table};
use crate::{Kind, Mode};
use std::num::NonZeroU32;

// #57; see README §"Control plane"
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
