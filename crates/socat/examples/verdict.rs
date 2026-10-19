// see crates/socat/CLAUDE.md §"Benchmark"
use std::io::{BufRead, Write};

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line.expect("stdin");
        let input: String =
            serde_json::from_str(&line).expect("one JSON string per line");
        let answer = match socat::Address::parse(&input) {
            Ok(address) => {
                serde_json::json!({ "ok": true, "canonical": address.to_string() })
            }
            Err(error) => serde_json::json!({
                "ok": false,
                "kind": error.kind.to_string(),
                "at": error.at,
            }),
        };
        writeln!(out, "{answer}").expect("stdout");
    }
}
