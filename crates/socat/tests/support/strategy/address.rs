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

pub(crate) fn local() -> impl Strategy<Value = Mode> {
    prop_oneof![
        Just(Mode::Connect),
        Just(Mode::Listen),
        Just(Mode::Sendto),
        Just(Mode::Recvfrom),
        Just(Mode::Recv),
        Just(Mode::Client),
    ]
}

pub(crate) fn every() -> impl Strategy<Value = Mode> {
    prop_oneof![local(), Just(Mode::Datagram)]
}

pub(crate) fn datagram() -> impl Strategy<Value = Mode> {
    prop_oneof![
        Just(Mode::Sendto),
        Just(Mode::Datagram),
        Just(Mode::Recvfrom),
        Just(Mode::Recv),
    ]
}

pub(crate) fn gram() -> impl Strategy<Value = Target> {
    let point = |ip: bool| -> BoxedStrategy<Point> {
        match ip {
            true => any::<u8>().prop_map(Point::Protocol).boxed(),
            false => port().prop_map(Point::Port).boxed(),
        }
    };
    (
        prop_oneof![Just(Gram::Udp), Just(Gram::Udplite), Just(Gram::Ip)],
        datagram(),
    )
        .prop_flat_map(move |(gram, mode)| {
            let addressed = matches!(mode, Mode::Sendto | Mode::Datagram);
            let host = match addressed {
                true => host().prop_map(Some).boxed(),
                false => Just(None).boxed(),
            };
            (host, point(gram == Gram::Ip)).prop_map(move |(host, point)| {
                Target::Gram {
                    gram,
                    mode,
                    host,
                    point,
                }
            })
        })
}

pub(crate) fn socket() -> impl Strategy<Value = Target> {
    (
        prop_oneof![mode(), datagram()],
        any::<u32>(),
        any::<u32>(),
        any::<u32>(),
        dalan(),
    )
        .prop_map(|(mode, domain, kind, protocol, address)| {
            let stream = matches!(mode, Mode::Connect | Mode::Listen);
            Target::Socket {
                mode,
                domain,
                kind: (!stream).then_some(kind),
                protocol,
                address,
            }
        })
}

pub(crate) fn item() -> impl Strategy<Value = Item> {
    let width = prop_oneof![
        Just(Width::I32),
        Just(Width::U32),
        Just(Width::I64),
        Just(Width::U64),
        Just(Width::I16),
        Just(Width::U16),
        Just(Width::I8),
        Just(Width::U8),
    ];
    prop_oneof![
        prop::collection::vec(any::<u8>(), 1..=6).prop_map(Item::Bytes),
        (width, any::<i64>()).prop_map(|(w, n)| {
            let n = i128::from(n);
            Item::Int(w, if w.fits(n) { n } else { n.rem_euclid(100) })
        }),
        "\\PC{0,6}".prop_map(Item::Text),
        any::<char>().prop_map(Item::Char),
    ]
}

pub(crate) fn dalan() -> impl Strategy<Value = Dalan> {
    prop::collection::vec(item(), 1..=4)
        .prop_map(|items| Dalan::new(items).expect("valid dalan"))
}

pub(crate) fn socks() -> impl Strategy<Value = Target> {
    (
        prop_oneof![Just(Socks::V4), Just(Socks::V4a), Just(Socks::V5)],
        mode(),
        host(),
        proptest::option::of(port()),
        host(),
        port(),
    )
        .prop_map(|(socks, mode, server, via, host, port)| {
            let v5 = socks == Socks::V5;
            Target::Socks {
                socks,
                mode: if v5 { mode } else { Mode::Connect },
                server,
                via: via.filter(|_| v5),
                host,
                port,
            }
        })
}

pub(crate) fn mq() -> impl Strategy<Value = Mq> {
    prop_oneof![
        Just(Mq::Read),
        Just(Mq::Receive),
        Just(Mq::Send),
        Just(Mq::Write),
        Just(Mq::Bidirectional),
        Just(Mq::Plain),
    ]
}

pub(crate) fn extra() -> impl Strategy<Value = Target> {
    prop_oneof![
        texts().prop_map(Target::Create),
        texts().prop_map(Target::System),
        proptest::option::of(texts()).prop_map(Target::Shell),
        any::<u32>().prop_map(Target::Fd),
        any::<u32>().prop_map(Target::Accept),
        Just(Target::Stdin),
        Just(Target::Stdout),
        Just(Target::Stderr),
        proptest::option::of(texts()).prop_map(Target::Pipe),
        Just(Target::Socketpair),
        (mq(), texts()).prop_map(|(mq, name)| Target::Mq {
            mq,
            name: Text::new(format!("/{}", name.as_str())).expect("queue"),
        }),
        proptest::option::of((any::<u32>(), 0u8..=32)).prop_map(|t| {
            Target::Tun(t.map(|(ip, b)| (Ipv4Addr::from(ip), b)))
        }),
        Just(Target::Readline),
        Just(Target::Stall),
        texts().prop_map(Target::Text),
        (host(), host(), port()).prop_map(|(proxy, host, port)| {
            Target::Proxy { proxy, host, port }
        }),
    ]
}

pub(crate) fn inet() -> impl Strategy<Value = Inet> {
    prop_oneof![
        (host(), port()).prop_map(|(host, port)| Inet::Connect { host, port }),
        (proptest::option::of(host()), port())
            .prop_map(|(host, port)| Inet::Listen { host, port }),
    ]
}

pub(crate) fn target() -> impl Strategy<Value = Target> {
    let net = prop_oneof![
        Just(Net::Tcp),
        Just(Net::Udp),
        Just(Net::Udplite),
        Just(Net::Sctp),
        Just(Net::Dccp),
    ];
    let web =
        prop_oneof![Just(Web::Ws), Just(Web::Wss), Just(Web::Webtransport)];
    prop_oneof![
        texts().prop_map(Target::Open),
        paths().prop_map(Target::Gopen),
        (local(), texts()).prop_map(|(mode, path)| Target::Unix { mode, path }),
        (local(), texts())
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
        gram(),
        socket(),
        socks(),
        extra(),
    ]
}

pub(crate) fn rest() -> impl Strategy<Value = Vec<Opt>> {
    let one = ("[a-z][a-z0-9_-]{0,10}", values())
        .prop_filter_map("reserved", |(n, v)| Opt::new(&n, v.as_deref()));
    prop::collection::vec(one, 0..4)
}

pub(crate) fn addresses() -> impl Strategy<Value = Address> {
    let access = proptest::option::of(prop_oneof![
        Just(Access::Read),
        Just(Access::Write)
    ]);
    (
        target(),
        rest(),
        any::<bool>(),
        any::<Option<NonZeroU32>>(),
        access,
    )
        .prop_map(|(target, rest, mux, cap, access)| {
            let mux = target.forks() && (mux || cap.is_some());
            let access = access.filter(|_| target.access());
            let options = Options::new(
                mux,
                cap.filter(|_| mux),
                access,
                Unchecked::new(rest),
            )
            .expect("valid options");
            Address::new(target, options).expect("valid address")
        })
}

pub(crate) fn loose() -> impl Strategy<Value = Target> {
    (
        target(),
        every(),
        any::<(bool, bool)>(),
        any::<u8>(),
        texts(),
    )
        .prop_map(|(t, m, (a, b), n, s)| match t {
            Target::Unix { path, .. } => Target::Unix { mode: m, path },
            Target::Abstract { name, .. } => Target::Abstract { mode: m, name },
            Target::Gram {
                gram, host, point, ..
            } => Target::Gram {
                gram,
                mode: m,
                host: match a {
                    true => host.xor(Some(Host::V4(Ipv4Addr::LOCALHOST))),
                    false => host,
                },
                point: match (b, point) {
                    (true, Point::Port(_)) => Point::Protocol(n),
                    (true, Point::Protocol(p)) => {
                        Point::Port(Port::Number(p.into()))
                    }
                    (false, p) => p,
                },
            },
            Target::Socket {
                domain,
                kind,
                protocol,
                address,
                ..
            } => Target::Socket {
                mode: m,
                domain,
                kind: match a {
                    true => kind.xor(Some(n.into())),
                    false => kind,
                },
                protocol,
                address,
            },
            Target::Socks {
                socks,
                server,
                via,
                host,
                port,
                ..
            } => Target::Socks {
                socks,
                mode: m,
                server,
                via: match a {
                    true => via.xor(Some(Port::Number(1080))),
                    false => via,
                },
                host,
                port,
            },
            Target::Mq { mq, .. } => Target::Mq {
                mq,
                name: match a {
                    true => s,
                    false => {
                        Text::new(format!("/{}", s.as_str())).expect("text")
                    }
                },
            },
            Target::Tun(Some((ip, _))) => Target::Tun(Some((ip, n))),
            t => t,
        })
}
