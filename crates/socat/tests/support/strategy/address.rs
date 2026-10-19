use super::*;

pub(crate) fn strings() -> impl Strategy<Value = String> {
    prop_oneof![
        "[ -~]{1,24}",
        "\\PC{1,12}",
        "[\\x00-\\x1f a-z!]{1,8}",
        "[a-z]{0,3}!![a-z]{0,3}",
    ]
}

pub(crate) fn texts() -> impl Strategy<Value = Text> {
    strings().prop_filter_map("empty", Text::new)
}

pub(crate) fn paths() -> impl Strategy<Value = Text> {
    prop_oneof![
        texts(),
        strings().prop_filter_map("empty", |s| Text::new(format!("/{s}"))),
    ]
}

pub(crate) fn values() -> impl Strategy<Value = Option<String>> {
    proptest::option::of(prop_oneof![
        Just(String::new()),
        "[ -~]{0,10}",
        "\\PC{0,8}",
        "[\\x00-\\x1f]{1,4}",
    ])
}

pub(crate) fn mode() -> impl Strategy<Value = Mode> {
    prop_oneof![Just(Mode::Connect), Just(Mode::Listen)]
}

pub(crate) fn inet() -> impl Strategy<Value = Inet> {
    prop_oneof![
        (host(), port()).prop_map(|(host, port)| Inet::Connect { host, port }),
        (proptest::option::of(host()), port())
            .prop_map(|(host, port)| Inet::Listen { host, port }),
    ]
}

pub(crate) fn target() -> impl Strategy<Value = Target> {
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

pub(crate) fn rest() -> impl Strategy<Value = Vec<Opt>> {
    let one = ("[a-z][a-z0-9_-]{0,10}", values())
        .prop_filter_map("reserved", |(n, v)| Opt::new(&n, v.as_deref()));
    prop::collection::vec(one, 0..4)
}

pub(crate) fn addresses() -> impl Strategy<Value = Address> {
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
