// see crates/socat/CLAUDE.md §"Benchmark"

mod corpus;
mod matrix;
mod oracle;
mod report;
mod tsv;

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

fn var(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| {
        panic!("{k} is unset; run crates/socat/peers/compare.sh")
    })
}

fn path(k: &str) -> PathBuf {
    PathBuf::from(var(k))
}

fn run(program: &PathBuf, args: &[&str]) -> String {
    let out = Command::new(program)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("{program:?}: {e}"));
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    text
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        if std::thread::current().name() != Some("tocat") {
            eprintln!("{info}");
        }
    }));
    let peers = path("PEERS");
    let oracles = oracle::Oracles {
        socat: path("ORACLE_SOCAT"),
        websocat: path("ORACLE_WEBSOCAT"),
        oittaa: path("ORACLE_OITTAA"),
        sandbox: path("SANDBOX"),
        versions: [
            "VERSION_SOCAT",
            "VERSION_WEBSOCAT",
            "VERSION_OITTAA",
            "VERSION_TOCAT",
        ]
        .map(var),
    };

    let mut corpus = corpus::Corpus::default();
    corpus.socat(&run(&oracles.socat, &["-hhh"]), "socat");
    corpus.oittaa(&run(&oracles.oittaa, &["-hhh"]));
    corpus.websocat(
        &run(&oracles.websocat, &["--help=full"]),
        &std::fs::read_to_string(path("WEBSOCAT_DOC"))
            .expect("websocat doc.md"),
    );
    let tests = run(&path("OITTAA_CORPUS"), &[&var("OITTAA_SRC")]);
    let tests: Vec<String> = tests.lines().filter_map(json).collect();
    corpus.lines(&tests.join("\n"), "oittaa:test");
    corpus.tocat(
        &std::fs::read_to_string(path("TOCAT_PARSE")).expect("tocat parse.rs"),
    );
    corpus.hand(
        &std::fs::read_to_string(peers.join("hand.txt")).expect("hand.txt"),
    );

    let entries = &corpus.entries;
    let verdicts = oracles.judge(entries);
    let mut accepted = [0usize; 4];
    for v in &verdicts {
        for (a, ok) in accepted.iter_mut().zip(v) {
            *a += usize::from(*ok);
        }
    }
    let file = peers.join("matrix.tsv");
    let matrix = matrix::Matrix::load(&file);
    let misses = matrix.check(entries, &verdicts, &oracles.versions);
    let blessed = std::env::var("BENCHMARK_BLESS").is_ok_and(|v| !v.is_empty());
    let shown = if blessed {
        let next = matrix.bless(entries, &verdicts, &oracles.versions);
        next.save(&file);
        next
    } else {
        matrix
    };
    let text = report::render(
        &shown,
        &misses,
        &oracles.versions,
        &std::env::var("UPSTREAM_SOCAT").unwrap_or_default(),
        &accepted,
        blessed,
    );
    let target = std::env::var("BENCHMARK_REPORT")
        .unwrap_or_else(|_| "/dev/stdout".into());
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&target)
        .unwrap_or_else(|e| panic!("{target}: {e}"));
    writeln!(f, "{text}").expect("write report");
    if !misses.is_empty() && !blessed {
        std::process::exit(1);
    }
}

fn json(line: &str) -> Option<String> {
    let inner = line.trim().strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'u' => {
                let hex: String = chars.by_ref().take(4).collect();
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
            }
            c => out.push(c),
        }
    }
    Some(out)
}
