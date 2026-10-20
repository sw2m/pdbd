// see README.md §"Benchmarks"
use socat::{Channel, Exclusion, Kind};
use std::io::{BufRead, Write};

fn class(kind: &Kind) -> &'static str {
    match kind {
        Kind::Excluded(Exclusion::Openssl) => "excluded-openssl",
        Kind::Excluded(Exclusion::Family(_)) => "excluded-family",
        Kind::Deferred(_) => "deferred",
        _ => "invalid",
    }
}

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line.expect("stdin");
        let input: String =
            serde_json::from_str(&line).expect("one JSON string per line");
        let answer = match Channel::parse(&input) {
            Ok(channel) => {
                serde_json::json!({ "ok": true, "canonical": channel.to_string() })
            }
            Err(error) => serde_json::json!({
                "ok": false,
                "class": class(&error.kind),
                "kind": error.kind.to_string(),
                "at": error.at,
            }),
        };
        writeln!(out, "{answer}").expect("stdout");
        out.flush().expect("stdout");
    }
}
