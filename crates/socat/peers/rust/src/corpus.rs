use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct Entry {
    pub text: String,
    pub family: String,
}

#[derive(Default)]
pub struct Corpus {
    pub entries: Vec<Entry>,
    seen: HashSet<String>,
}

struct Head {
    name: String,
    signature: String,
    groups: Vec<String>,
    alias: Option<String>,
}

struct Opt {
    name: String,
    groups: Vec<String>,
    kind: String,
    alias: Option<String>,
}

struct Help {
    heads: Vec<Head>,
    opts: Vec<Opt>,
}

impl Corpus {
    pub fn push(&mut self, text: &str, family: &str) {
        if text.is_empty()
            || text.contains('\0')
            || !self.seen.insert(text.into())
        {
            return;
        }
        self.entries.push(Entry {
            text: text.into(),
            family: family.into(),
        });
    }

    pub fn socat(&mut self, help: &str, origin: &str) {
        let help = Help::parse(help);
        for head in &help.heads {
            let target = help.resolve(head);
            let form = match head.name.as_str() {
                "<filename>" => "/tmp/f".to_string(),
                "<num>" => "3".to_string(),
                name => format!("{name}{}", fill(&target.signature)),
            };
            let family = format!("{origin}:type:{}", family(&target.name));
            self.push(&form, &family);
        }
        let hosts = help.hosts();
        for opt in &help.opts {
            let target = help.option(opt);
            let forked =
                matches!(normal(&target.name).as_str(), "fork" | "maxchildren");
            let fits = |groups: &Vec<String>| {
                target.groups.iter().any(|g| groups.contains(g))
            };
            let host = hosts
                .iter()
                .filter(|(g, _)| fits(g))
                .find(|(_, form)| !forked || form.contains("LISTEN"))
                .or_else(|| hosts.iter().find(|(g, _)| fits(g)));
            let Some((_, form)) = host else { continue };
            let entry = match value(&target.kind) {
                Some(v) => format!("{form},{}={v}", opt.name),
                None => format!("{form},{}", opt.name),
            };
            let group = target.groups.first().map_or("-", String::as_str);
            self.push(&entry, &format!("{origin}:option:{group}"));
        }
    }

    pub fn oittaa(&mut self, help: &str) {
        let mut section = "";
        let mut heading = String::new();
        let mut types = 0;
        let mut options = 0;
        let mut signatures: std::collections::HashMap<String, String> =
            Default::default();
        for line in help.lines() {
            match line {
                "Address types:" => {
                    section = "type";
                    continue;
                }
                "Address options:" => {
                    section = "option";
                    continue;
                }
                l if !l.starts_with(' ') && !l.is_empty() => section = "",
                _ => {}
            }
            if section.is_empty() {
                continue;
            }
            if let Some(h) =
                line.strip_prefix("  ").filter(|h| !h.starts_with(' '))
            {
                heading = h.trim().to_lowercase();
                continue;
            }
            let Some(rest) = line.strip_prefix("    ") else {
                continue;
            };
            let mut words = rest.splitn(2, char::is_whitespace);
            let first = words.next().unwrap_or("");
            let desc = words.next().unwrap_or("").trim().to_lowercase();
            if first.is_empty() || first.starts_with('-') && section == "option"
            {
                continue;
            }
            if section == "type" {
                let cut = first.find([':', '[']).unwrap_or(first.len());
                let (name, mut signature) =
                    (&first[..cut], first[cut..].to_string());
                let target = desc
                    .strip_prefix("alias of ")
                    .or_else(|| desc.strip_prefix("same as "))
                    .and_then(|t| t.split_whitespace().next())
                    .map(str::to_ascii_uppercase);
                if signature.is_empty() {
                    if let Some(sig) = target.and_then(|t| signatures.get(&t)) {
                        signature = sig.clone();
                    }
                }
                signatures.insert(name.to_string(), signature.clone());
                let form = format!("{name}{}", fill(&signature));
                self.push(
                    &form,
                    &format!("oittaa:type:{}", family(&first[..cut])),
                );
                types += 1;
            } else {
                let valued = [
                    "time", "size", "limit", "count", "number", "second",
                    "max", "path", "file", "name", "level", "mode", "=",
                ]
                .iter()
                .any(|w| desc.contains(w));
                let opt = if valued {
                    format!("{first}=1")
                } else {
                    first.to_string()
                };
                self.push(
                    &format!("{},{opt}", host(&heading)),
                    &format!("oittaa:option:{heading}"),
                );
                options += 1;
            }
        }
        if types == 0 || options == 0 {
            panic!("oittaa -hhh: no address types or options parsed");
        }
    }

    pub fn lines(&mut self, text: &str, family: &str) {
        for line in text.lines().filter(|l| !l.is_empty()) {
            self.push(line, family);
        }
    }

    pub fn hand(&mut self, text: &str) {
        let mut family = String::from("hand:misc");
        for line in text.lines() {
            if let Some(section) = line.strip_prefix("# ") {
                family = format!("hand:{}", section.trim());
            } else if !line.is_empty() {
                self.push(&crate::tsv::unescape(line), &family);
            }
        }
    }

    pub fn websocat(&mut self, help: &str, doc: &str) {
        let aliases = aliases(doc);
        let known: Vec<String> = help
            .lines()
            .filter_map(|l| l.strip_prefix('\t'))
            .filter_map(|l| l.split('\t').next())
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .chain(aliases.iter().flat_map(|(_, a)| a.iter().cloned()))
            .collect();
        let examples: Vec<String> = examples(doc)
            .into_iter()
            .filter(|e| {
                e == "-"
                    || known
                        .iter()
                        .any(|n| n != "-" && e.starts_with(n.as_str()))
            })
            .collect();
        let mut section = "";
        for line in help.lines() {
            if line.starts_with("Full list of address types") {
                section = "type";
                continue;
            }
            if line.starts_with("Full list of overlays") {
                section = "overlay";
                continue;
            }
            let Some(rest) = line.strip_prefix('\t') else {
                continue;
            };
            if section.is_empty() {
                continue;
            }
            let mut parts = rest.splitn(2, '\t');
            let name = parts.next().unwrap_or("").trim();
            let desc = parts.next().unwrap_or("").to_lowercase();
            if name.is_empty() {
                continue;
            }
            let form = examples
                .iter()
                .find(|e| e.starts_with(name) && e.len() > name.len())
                .cloned()
                .unwrap_or_else(|| guess(name, &desc, section));
            let family = format!("websocat:{section}");
            self.push(&form, &family);
            for alias in aliases
                .iter()
                .filter(|(n, _)| n == name)
                .flat_map(|(_, a)| a)
            {
                self.push(&format!("{alias}{}", &form[name.len()..]), &family);
            }
        }
        for e in &examples {
            self.push(e, "websocat:example");
        }
    }

    pub fn tocat(&mut self, source: &str) {
        let Some(start) = source.find("match normalize(scheme)") else {
            panic!("tocat: scheme table not found in endpoint/parse.rs");
        };
        let block = &source[start..];
        let end = block.find("other =>").expect("tocat: end of scheme table");
        let mut names = Vec::new();
        for line in block[..end].lines().skip(1) {
            let t = line.trim_start();
            if !(t.starts_with('"') || t.starts_with('|')) {
                continue;
            }
            let head = t.split("=>").next().unwrap_or("");
            for part in head.split('"').skip(1).step_by(2) {
                names.push(part.to_string());
            }
        }
        if names.is_empty() {
            panic!("tocat: no schemes scraped");
        }
        self.push("-", "tocat:scheme");
        for name in names {
            let body = scheme(&name);
            self.push(&format!("{name}:{body}"), "tocat:scheme");
        }
    }
}

impl Help {
    fn parse(text: &str) -> Self {
        let mut heads = Vec::new();
        let mut opts = Vec::new();
        let mut section = "";
        for line in text.lines() {
            match line.trim() {
                "address-head:" => {
                    section = "head";
                    continue;
                }
                "opts:" | "opt:" => {
                    section = "opt";
                    continue;
                }
                _ => {}
            }
            if !line.starts_with("      ") {
                continue;
            }
            let mut words = line.split_whitespace();
            let Some(first) = words.next() else { continue };
            let rest: Vec<&str> = words.collect();
            let alias = rest
                .last()
                .filter(|_| rest.contains(&"alias"))
                .map(|s| s.to_string());
            let field = |key: &str| {
                rest.iter()
                    .find_map(|w| w.strip_prefix(key))
                    .map(|v| v.to_string())
                    .unwrap_or_default()
            };
            let groups: Vec<String> = field("groups=")
                .split(',')
                .filter(|g| !g.is_empty())
                .map(String::from)
                .collect();
            match section {
                "head" => {
                    let cut = first.find([':', '[']).unwrap_or(first.len());
                    heads.push(Head {
                        name: first[..cut].into(),
                        signature: first[cut..].into(),
                        groups,
                        alias,
                    });
                }
                "opt" => opts.push(Opt {
                    name: first.into(),
                    groups,
                    kind: field("type="),
                    alias,
                }),
                _ => {}
            }
        }
        if heads.is_empty() || opts.is_empty() {
            panic!("socat -hhh: no address types or options parsed");
        }
        Self { heads, opts }
    }

    fn resolve<'a>(&'a self, head: &'a Head) -> &'a Head {
        let mut h = head;
        for _ in 0..8 {
            let Some(target) = &h.alias else { return h };
            match self.heads.iter().find(|x| &x.name == target) {
                Some(next) => h = next,
                None => return h,
            }
        }
        h
    }

    fn option<'a>(&'a self, opt: &'a Opt) -> &'a Opt {
        let mut o = opt;
        for _ in 0..8 {
            let Some(target) = &o.alias else { return o };
            match self.opts.iter().find(|x| &x.name == target) {
                Some(next) => o = next,
                None => return o,
            }
        }
        o
    }

    fn hosts(&self) -> Vec<(Vec<String>, String)> {
        let mut all: Vec<(u8, Vec<String>, String)> = self
            .heads
            .iter()
            .filter(|h| h.alias.is_none())
            .map(|h| {
                let form = format!("{}{}", h.name, fill(&h.signature));
                let ours = socat::Address::parse(&form).is_ok();
                let listen = h.name.contains("LISTEN");
                let rank = u8::from(!ours) * 2 + u8::from(listen);
                (rank, h.groups.clone(), form)
            })
            .collect();
        all.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.cmp(&b.2)));
        all.into_iter().map(|(_, g, f)| (g, f)).collect()
    }
}

fn host(heading: &str) -> &'static str {
    let has = |w: &str| heading.contains(w);
    if has("listen") || has("security") {
        "TCP-LISTEN:1"
    } else if has("file") {
        "OPEN:f"
    } else if has("exec") {
        "EXEC:true"
    } else if has("pty") || has("termios") {
        "PTY"
    } else if has("transfer") {
        "STDIO"
    } else if has("websocket") {
        "WS-CONNECT:127.0.0.1:1"
    } else if has("tls") {
        "OPENSSL:127.0.0.1:1"
    } else if has("proxy") {
        "PROXY:127.0.0.1:example.com:1"
    } else if has("posix") {
        "POSIXMQ-READ:/q"
    } else if has("tun") {
        "TUN"
    } else {
        "TCP-CONNECT:127.0.0.1:1"
    }
}

fn normal(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, '-' | '_'))
        .collect::<String>()
        .to_ascii_lowercase()
}

fn family(name: &str) -> String {
    let root = name.split('-').next().unwrap_or(name);
    let root = root.trim_end_matches(|c: char| c.is_ascii_digit());
    if root.is_empty() {
        name.to_string()
    } else {
        root.to_string()
    }
}

fn fill(signature: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in signature.chars() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let mut filled = String::new();
    let mut rest = out.as_str();
    while let Some(open) = rest.find('<') {
        filled.push_str(&rest[..open]);
        let close = rest[open..].find('>').map_or(rest.len(), |i| open + i);
        filled.push_str(sample(&rest[open + 1..close]));
        rest = &rest[(close + 1).min(rest.len())..];
    }
    filled.push_str(rest);
    filled
}

fn sample(kind: &str) -> &'static str {
    match kind {
        "host" | "target-host" | "listen-host" | "remote-address"
        | "local-address" | "ip-addr" | "address" | "proxy-server"
        | "socks-server" => "127.0.0.1",
        "port" | "target-port" | "listen-port" | "socks-port" => "1",
        "filename" | "file" | "lockfile" | "logfile" => "f",
        "command-line" | "shell-command" => "true",
        "cid" => "2",
        "fdnum" => "3",
        "protocol" => "255",
        "domain" | "type" | "num" => "1",
        "interface" => "lo",
        "mqname" => "/q",
        "bits" => "24",
        _ => "x",
    }
}

fn value(kind: &str) -> Option<String> {
    let one = |k: &str| match k {
        "STRING" | "STRING-NULL" | "GENERIC" | "BIN" | "IP4NAME" => "x",
        "IP4SOCK" | "STRUCT-IP_MREQN" | "IP-MREQ-SOURCE" => "127.0.0.1",
        _ => "1",
    };
    match kind {
        "BOOL" | "CONST" | "" => None,
        "INT[3]" => Some("1:1:1".into()),
        k if k.contains(':') => {
            Some(k.split(':').map(one).collect::<Vec<_>>().join(":"))
        }
        k => Some(one(k).into()),
    }
}

fn examples(doc: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in doc.lines() {
        let Some(cmd) = line.trim_start().strip_prefix("websocat ") else {
            continue;
        };
        if !line.starts_with("    ") {
            continue;
        }
        for word in words(cmd) {
            if word.starts_with('-') && word != "-" {
                continue;
            }
            if word.contains(':') || word == "-" {
                out.push(word);
            }
        }
    }
    out
}

fn words(cmd: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote = None;
    let mut any = false;
    for c in cmd.chars() {
        match (quote, c) {
            (None, '\'' | '"') => {
                quote = Some(c);
                any = true;
            }
            (Some(q), c) if c == q => quote = None,
            (None, c) if c.is_whitespace() => {
                if any {
                    out.push(std::mem::take(&mut cur));
                }
                any = false;
            }
            (None, '|' | '&' | ';' | '>' | '<') => break,
            (_, c) => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}

fn aliases(doc: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let mut name = None;
    for line in doc.lines() {
        if let Some(rest) = line.strip_prefix("### `") {
            name = rest.split('`').next().map(String::from);
        } else if let (Some(n), Some(rest)) =
            (&name, line.strip_prefix("Aliases:"))
        {
            let list = rest
                .split('`')
                .skip(1)
                .step_by(2)
                .map(String::from)
                .collect();
            out.push((n.clone(), list));
        }
    }
    out
}

fn guess(name: &str, desc: &str, section: &str) -> String {
    if section == "overlay" {
        return format!("{name}tcp:127.0.0.1:1");
    }
    if name.ends_with("://") {
        return format!("{name}127.0.0.1/");
    }
    let arg = if desc.contains("abstract") {
        "name"
    } else if desc.contains("unix") {
        "/tmp/s"
    } else if desc.contains("port") || desc.contains("socket address") {
        "127.0.0.1:1"
    } else if desc.contains("file") || desc.contains("path") {
        "/tmp/f"
    } else if desc.contains("command") || desc.contains("program") {
        "true"
    } else {
        ""
    };
    format!("{name}{arg}")
}

fn scheme(name: &str) -> &'static str {
    let has = |w: &str| name.contains(w);
    if has("exec") || has("system") {
        "true"
    } else if has("unix") || has("uds") || has("seqp") {
        "/tmp/s"
    } else if has("file") || has("open") || has("pipe") || has("fifo") {
        "f"
    } else if has("tty") || has("serial") {
        "/dev/ttyS0"
    } else if has("proxy") || has("socks") {
        "127.0.0.1:1:example.com:2"
    } else if has("ws") || has("websocket") {
        "127.0.0.1:1/x"
    } else if has("tcp")
        || has("udp")
        || has("listen")
        || has("connect")
        || has("tls")
        || has("ssl")
        || has("noise")
    {
        "127.0.0.1:1"
    } else {
        ""
    }
}
