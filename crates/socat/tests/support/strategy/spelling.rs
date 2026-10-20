use super::*;

pub(crate) fn modal(
    root: &str,
    mode: Mode,
    bare: &[(&str, Mode)],
) -> Vec<String> {
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
        mode => vec![format!("{root}-{mode}")],
    };
    all.extend(
        bare.iter()
            .filter(|(_, m)| *m == mode)
            .map(|(b, _)| b.to_string()),
    );
    all
}

pub(crate) fn synonyms(t: &Target) -> Vec<String> {
    let modal = |root, bare| modal(root, t.mode().expect("modal"), bare);
    match t {
        Target::Open(_) => vec!["OPEN".into(), "FILE".into()],
        Target::Gopen(p) if p.as_str().contains('/') => {
            vec!["GOPEN".into(), String::new()]
        }
        Target::Gopen(_) => vec!["GOPEN".into()],
        Target::Unix { .. } => {
            modal("UNIX", &[("UNIX", Mode::Client), ("LOCAL", Mode::Connect)])
        }
        Target::Abstract { .. } => {
            modal("ABSTRACT", &[("ABSTRACT", Mode::Client)])
        }
        Target::Vsock(_) => modal("VSOCK", &[("VSOCK", Mode::Connect)]),
        Target::Net { net: Net::Tcp, .. } => {
            modal("TCP", &[("TCP", Mode::Connect), ("INET", Mode::Connect)])
        }
        Target::Net { net, .. } => {
            let root = net.to_string();
            modal(&root, &[(&root, Mode::Connect)])
        }
        Target::Gram { gram: Gram::Ip, .. } => {
            modal("IP", &[("IP", Mode::Sendto)])
        }
        Target::Gram { gram, .. } => modal(&gram.to_string(), &[]),
        Target::Socket { .. } => modal("SOCKET", &[]),
        Target::Socks {
            socks: Socks::V5, ..
        } => modal("SOCKS5", &[("SOCKS5", Mode::Connect)]),
        Target::Socks {
            socks: Socks::V4, ..
        } => vec!["SOCKS4".into(), "SOCKS".into()],
        Target::Socks { socks, .. } => vec![socks.to_string()],
        Target::Proxy { .. } => vec!["PROXY".into(), "PROXY-CONNECT".into()],
        Target::Create(_) => vec!["CREATE".into(), "CREAT".into()],
        Target::System(_) => vec!["SYSTEM".into()],
        Target::Shell(_) => vec!["SHELL".into()],
        Target::Fd(_) => vec!["FD".into(), String::new()],
        Target::Accept(_) => vec!["ACCEPT-FD".into(), "ACCEPT".into()],
        Target::Stdin => vec!["STDIN".into()],
        Target::Stdout => vec!["STDOUT".into()],
        Target::Stderr => vec!["STDERR".into()],
        Target::Pipe(_) => vec!["PIPE".into(), "ECHO".into(), "FIFO".into()],
        Target::Socketpair => vec!["SOCKETPAIR".into()],
        Target::Mq {
            mq: Mq::Receive, ..
        } => vec!["POSIXMQ-RECEIVE".into(), "POSIXMQ-RECV".into()],
        Target::Mq { mq, .. } => vec![mq.to_string()],
        Target::Tun(_) => vec!["TUN".into()],
        Target::Readline => vec!["READLINE".into()],
        Target::Stall => vec!["STALL".into()],
        Target::Text(_) => vec!["TEXT".into()],
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
        inet:
            Inet::Connect {
                host,
                port: port @ Port::Number(_),
            },
        resource,
    } = t
    else {
        return None;
    };
    if matches!(host, Host::V6(_, Some(_))) {
        return None;
    }
    let scheme = match web {
        Web::Ws => "ws",
        Web::Wss => "wss",
        Web::Webtransport => "https",
    };
    let printed = Address::new(
        Target::Web {
            web: *web,
            inet: Inet::Connect {
                host: host.clone(),
                port: port.clone(),
            },
            resource: resource.clone(),
        },
        Options::default(),
    )
    .expect("address")
    .to_string();
    let (_, rest) = printed.split_once(':').expect("keyword");
    match host {
        Host::Name(name) if !name.as_str().is_ascii() => {
            use idna::uts46::{AsciiDenyList, DnsLength, Hyphens, Uts46};
            let label = Uts46::new()
                .to_ascii(
                    name.as_str().as_bytes(),
                    AsciiDenyList::STD3,
                    Hyphens::CheckFirstLast,
                    DnsLength::VerifyAllowRootDot,
                )
                .expect("a-label");
            let rest = rest.strip_prefix(name.as_str()).expect("host first");
            Some(format!("{scheme}://{label}{rest}"))
        }
        _ => Some(format!("{scheme}://{rest}")),
    }
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
