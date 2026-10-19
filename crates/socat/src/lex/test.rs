use crate::fixture::{exec, opt, parse, plain, table, text};
use crate::strategy::addresses;
use crate::{Address, Error, Kind, Mode, Target};
use proptest::prelude::*;

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

// #57
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
    assert_eq!(exec(r"EXEC:a\vb"), Target::Exec(text("a\u{b}b")));
    assert_eq!(exec(r#"EXEC:"\v""#), Target::Exec(text("\u{b}")));
    table(&[
        (r"EXEC:a\0b", 6, Kind::Nul),
        (r#"EXEC:"a\0""#, 7, Kind::Nul),
        (r"FILE:/x,v=\0", 10, Kind::Nul),
    ]);
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

proptest! {
    // #57
    #[test]
    fn unterminated(a in addresses(), body in "[^']{0,8}") {
        let s = format!("{a},x='{body}");
        let at = s.len() - body.len() - 1;
        prop_assert_eq!(Address::parse(&s), Err(Error::new(at, Kind::Quote)));
    }

    // #57
    #[test]
    fn unclosed(a in addresses(), open in "[\\[({]", body in "[a-z]{0,8}") {
        let s = format!("{a},x={open}{body}");
        let at = s.len() - body.len() - 1;
        prop_assert_eq!(Address::parse(&s), Err(Error::new(at, Kind::Bracket)));
    }

    // #57
    #[test]
    fn duals(a in addresses(), tail in "[a-z]{0,6}") {
        let s = format!("{a},x=y!!{tail}");
        let at = s.len() - tail.len() - 2;
        prop_assert_eq!(Address::parse(&s), Err(Error::new(at, Kind::Dual)));
    }
}
