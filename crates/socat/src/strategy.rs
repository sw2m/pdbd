use crate::fixture::plain;
use crate::lex::Quoted;
use crate::{
    Address, Host, Inet, Mode, Name, Net, Opt, Options, Port, Resource,
    Service, Target, Text, Unchecked, Vsock, Web, Zone,
};
use proptest::prelude::*;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::num::NonZeroU32;

pub(crate) fn host() -> impl Strategy<Value = Host> {
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

pub(crate) fn port() -> impl Strategy<Value = Port> {
    prop_oneof![
        any::<u16>().prop_map(Port::Number),
        "[a-z][a-z0-9-]{0,8}"
            .prop_filter_map("not a service", |s| Service::new(&s))
            .prop_map(Port::Service),
    ]
}

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

pub(crate) fn resource() -> impl Strategy<Value = Resource> {
    prop_oneof![
        Just(Resource::default()),
        "/[!-~]{0,12}".prop_filter_map("invalid", |r| Resource::new(&r)),
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

pub(crate) fn modal(root: &str, mode: Mode, bare: &[&str]) -> Vec<String> {
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

pub(crate) fn synonyms(t: &Target) -> Vec<String> {
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

pub(crate) fn scramble(s: &str, mask: u64) -> String {
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
pub(crate) struct Spelling {
    pub(crate) synonym: usize,
    pub(crate) mask: u64,
    pub(crate) fork: bool,
    pub(crate) swap: bool,
    pub(crate) cap: usize,
    pub(crate) url: bool,
    pub(crate) empty: bool,
}

pub(crate) fn spelling() -> impl Strategy<Value = Spelling> {
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

pub(crate) fn url(t: &Target) -> Option<String> {
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

pub(crate) fn respell(a: &Address, sp: &Spelling) -> String {
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

pub(crate) fn check(s: &str) -> Result<(), TestCaseError> {
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
pub(crate) enum Edit {
    Insert(usize, u8),
    Delete(usize),
    Substitute(usize, u8),
}

impl Edit {
    pub(crate) fn apply(&self, s: &str) -> Vec<u8> {
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

pub(crate) fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        (any::<usize>(), any::<u8>()).prop_map(|(i, b)| Edit::Insert(i, b)),
        any::<usize>().prop_map(Edit::Delete),
        (any::<usize>(), any::<u8>()).prop_map(|(i, b)| Edit::Substitute(i, b)),
    ]
}

pub(crate) fn keywords() -> Vec<String> {
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
