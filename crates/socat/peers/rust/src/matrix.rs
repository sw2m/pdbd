use crate::corpus::Entry;
use crate::oracle::NAMES;
use crate::tsv::{escape, unescape};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Supported,
    Rejected,
    Alternative,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub entry: String,
    pub family: String,
    pub verdicts: Vec<(bool, String)>,
    pub class: Class,
    pub ours: String,
}

#[derive(Default)]
pub struct Matrix {
    pub rows: BTreeMap<String, Row>,
}

pub struct Miss {
    pub entry: String,
    pub family: String,
    pub reason: String,
}

const HEADER: &str =
    "entry\tfamily\tsocat\twebsocat\toittaa\ttocat\tclass\tours";

impl Class {
    fn parse(s: &str) -> Self {
        match s {
            "supported" => Self::Supported,
            "rejected" => Self::Rejected,
            "alternative" => Self::Alternative,
            _ => panic!("matrix: unknown class {s:?}"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Rejected => "rejected",
            Self::Alternative => "alternative",
        }
    }
}

fn verdict(ok: bool) -> &'static str {
    if ok {
        "ok"
    } else {
        "err"
    }
}

fn ours(s: &str) -> Result<String, String> {
    socat::Address::parse(s)
        .map(|a| a.to_string())
        .map_err(|e| e.to_string())
}

impl Row {
    fn fresh(e: &Entry, verdicts: &[bool; 4], versions: &[String; 4]) -> Self {
        let (class, ours) = match ours(&e.text) {
            Ok(c) => (Class::Supported, c),
            Err(_) => match alternative(&e.text) {
                Some(a) => (Class::Alternative, a),
                None => (Class::Rejected, "-".into()),
            },
        };
        Self {
            entry: e.text.clone(),
            family: e.family.clone(),
            verdicts: stamp(verdicts, versions),
            class,
            ours,
        }
    }

    fn drift(&self) -> Option<String> {
        match (ours(&self.entry), self.class) {
            (Ok(c), Class::Supported) if c != self.ours => {
                Some(format!("ours prints `{c}`, recorded `{}`", self.ours))
            }
            (Ok(_), Class::Supported) => None,
            (Ok(c), class) => Some(format!(
                "ours now accepts it (prints `{c}`), recorded {}",
                class.name()
            )),
            (Err(e), Class::Supported) => {
                Some(format!("ours rejects it ({e}), recorded supported"))
            }
            (Err(_), Class::Alternative) => match ours(&self.ours) {
                Ok(_) => None,
                Err(e) => Some(format!(
                    "the recorded alternative `{}` no longer parses ({e})",
                    self.ours
                )),
            },
            (Err(_), Class::Rejected) => None,
        }
    }
}

fn stamp(verdicts: &[bool; 4], versions: &[String; 4]) -> Vec<(bool, String)> {
    verdicts
        .iter()
        .zip(versions)
        .map(|(v, n)| (*v, n.clone()))
        .collect()
}

impl Matrix {
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let mut rows = BTreeMap::new();
        for line in text.lines().skip(1).filter(|l| !l.is_empty()) {
            let f: Vec<&str> = line.split('\t').collect();
            assert_eq!(f.len(), 8, "matrix: malformed row {line:?}");
            let verdicts = f[2..6]
                .iter()
                .map(|c| {
                    let (v, n) =
                        c.split_once('@').expect("matrix: verdict@version");
                    (v == "ok", n.to_string())
                })
                .collect();
            let entry = unescape(f[0]);
            rows.insert(
                entry.clone(),
                Row {
                    entry,
                    family: f[1].into(),
                    verdicts,
                    class: Class::parse(f[6]),
                    ours: unescape(f[7]),
                },
            );
        }
        Self { rows }
    }

    pub fn save(&self, path: &Path) {
        let mut rows: Vec<&Row> = self.rows.values().collect();
        rows.sort_by(|a, b| {
            a.family.cmp(&b.family).then_with(|| a.entry.cmp(&b.entry))
        });
        let mut out = String::from(HEADER);
        out.push('\n');
        for r in rows {
            let cells: Vec<String> = r
                .verdicts
                .iter()
                .map(|(v, n)| format!("{}@{n}", verdict(*v)))
                .collect();
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\n",
                escape(&r.entry),
                r.family,
                cells.join("\t"),
                r.class.name(),
                escape(&r.ours)
            ));
        }
        std::fs::write(path, out).expect("write matrix");
    }

    pub fn check(
        &self,
        entries: &[Entry],
        verdicts: &[[bool; 4]],
        versions: &[String; 4],
    ) -> Vec<Miss> {
        let mut misses = Vec::new();
        let miss = |e: &str, f: &str, reason: String| Miss {
            entry: e.into(),
            family: f.into(),
            reason,
        };
        for (e, v) in entries.iter().zip(verdicts) {
            let Some(row) = self.rows.get(&e.text) else {
                misses.push(miss(
                    &e.text,
                    &e.family,
                    "new corpus entry".into(),
                ));
                continue;
            };
            for (i, ok) in v.iter().enumerate() {
                let (was, at) = &row.verdicts[i];
                if was != ok {
                    misses.push(miss(
                        &e.text,
                        &e.family,
                        format!(
                            "{}: recorded {}@{at}, now {}@{}",
                            NAMES[i],
                            verdict(*was),
                            verdict(*ok),
                            versions[i]
                        ),
                    ));
                }
            }
            if let Some(reason) = row.drift() {
                misses.push(miss(&e.text, &e.family, reason));
            }
        }
        let current: std::collections::HashSet<&str> =
            entries.iter().map(|e| e.text.as_str()).collect();
        for row in self
            .rows
            .values()
            .filter(|r| !current.contains(r.entry.as_str()))
        {
            misses.push(miss(
                &row.entry,
                &row.family,
                "no longer in the corpus".into(),
            ));
        }
        misses
    }

    pub fn bless(
        &self,
        entries: &[Entry],
        verdicts: &[[bool; 4]],
        versions: &[String; 4],
    ) -> Self {
        let mut rows = BTreeMap::new();
        for (e, v) in entries.iter().zip(verdicts) {
            let row = match self.rows.get(&e.text) {
                Some(old) if old.drift().is_none() => Row {
                    verdicts: stamp(v, versions),
                    family: e.family.clone(),
                    ..old.clone()
                },
                _ => Row::fresh(e, v, versions),
            };
            rows.insert(e.text.clone(), row);
        }
        Self { rows }
    }
}

fn alternative(s: &str) -> Option<String> {
    let cut = s.find([':', ',']).unwrap_or(s.len());
    let (keyword, rest) = s.split_at(cut);
    let upper = keyword.to_ascii_uppercase();
    let key: String =
        upper.chars().filter(|c| !matches!(c, '-' | '_')).collect();
    let pinned = |root: &str| {
        let tail = upper.strip_prefix(root)?;
        let tail = tail.strip_prefix(['4', '6'])?;
        Some(format!("{root}{tail}"))
    };
    let swap = match key.as_str() {
        "TCPCONNECT" | "CONNECT" => Some("TCP-CONNECT".into()),
        "TCPLISTEN" | "LISTEN" => Some("TCP-LISTEN".into()),
        "UDPCONNECT" => Some("UDP-CONNECT".into()),
        "UDPLISTEN" => Some("UDP-LISTEN".into()),
        "UDS" | "UDSCONNECT" | "UNIXCONNECT" => Some("UNIX-CONNECT".into()),
        "UDSLISTEN" | "UNIXLISTEN" => Some("UNIX-LISTEN".into()),
        "WS" | "WEBSOCKET" => Some("WS-CONNECT".into()),
        "WSLISTEN" | "WEBSOCKETLISTEN" => Some("WS-LISTEN".into()),
        "WSS" | "WEBSOCKETS" => Some("WSS-CONNECT".into()),
        "WSSLISTEN" | "WEBSOCKETSLISTEN" => Some("WSS-LISTEN".into()),
        "ABSTRACT" => Some("ABSTRACT-CONNECT".into()),
        "ABSTRACTLISTEN" => Some("ABSTRACT-LISTEN".into()),
        "THREADEDSTDIO" | "ASYNCSTDIO" | "STDIO" | "PTY"
            if rest == ":" || rest.is_empty() =>
        {
            return ours(if key == "PTY" { "PTY" } else { "STDIO" }).ok();
        }
        "QUIC" | "QUICCONNECT" if !rest.contains(',') => {
            let target = rest.strip_prefix(':')?;
            let text = socat::Text::new(format!(
                "quicat stdio quic-active-connect://{target}"
            ))?;
            let exec = socat::Address::new(
                socat::Target::Exec(text),
                Default::default(),
            )
            .ok()?;
            return ours(&exec.to_string()).ok();
        }
        "UNIXCLIENT" => Some("UNIX-CONNECT".into()),
        "ABSTRACTCLIENT" => Some("ABSTRACT-CONNECT".into()),
        "VSOCK" => Some("VSOCK-CONNECT".into()),
        "INET" | "INET4" | "INET6" => Some("TCP-CONNECT".into()),
        "INETL" | "INETLISTEN" | "INET4L" | "INET4LISTEN" | "INET6L"
        | "INET6LISTEN" => Some("TCP-LISTEN".into()),
        "OPENSSL" | "OPENSSLCONNECT" | "SSL" | "TLS" | "TLSCONNECT"
        | "SSLCONNECT"
            if !rest.contains(',') =>
        {
            let target = rest.strip_prefix(':')?;
            let text = socat::Text::new(format!(
                "openssl s_client -quiet -connect {target}"
            ))?;
            let exec = socat::Address::new(
                socat::Target::Exec(text),
                Default::default(),
            )
            .ok()?;
            return ours(&exec.to_string()).ok();
        }
        "TTY" | "SERIAL" => Some("GOPEN".into()),
        "SHC" | "CMD" | "SYSTEM" => {
            let (cmd, opts) = rest
                .strip_prefix(':')?
                .split_once(',')
                .map_or((rest.strip_prefix(':')?, ""), |(c, o)| (c, o));
            let text = socat::Text::new(format!(
                "sh -c '{}'",
                cmd.replace('\'', r"'\''")
            ))?;
            let exec = socat::Address::new(
                socat::Target::Exec(text),
                Default::default(),
            )
            .ok()?;
            let opts = if opts.is_empty() {
                String::new()
            } else {
                format!(",{opts}")
            };
            return ours(&format!("{exec}{opts}")).ok();
        }
        _ => ["TCP", "UDP"].iter().find_map(|r| pinned(r)),
    }?;
    ours(&format!("{swap}{rest}")).ok()
}
