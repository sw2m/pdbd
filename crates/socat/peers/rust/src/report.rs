use crate::matrix::{Class, Matrix, Miss};
use crate::oracle::NAMES;
use std::collections::BTreeMap;
use std::fmt::Write;

#[derive(Default)]
struct Tally {
    supported: usize,
    alternative: usize,
    rejected: usize,
    typed: usize,
}

impl Tally {
    fn add(&mut self, class: Class) {
        match class {
            Class::Supported => self.supported += 1,
            Class::Alternative => self.alternative += 1,
            Class::Rejected => self.rejected += 1,
        }
    }

    fn total(&self) -> usize {
        self.supported + self.alternative + self.rejected
    }
}

fn typed(entry: &str) -> bool {
    entry.split(',').skip(1).any(|o| {
        let name: String = o
            .split('=')
            .next()
            .unwrap_or("")
            .chars()
            .filter(|c| !matches!(c, '-' | '_'))
            .collect::<String>()
            .to_ascii_lowercase();
        matches!(name.as_str(), "mux" | "fork" | "maxchildren")
    })
}

pub fn render(
    matrix: &Matrix,
    misses: &[Miss],
    versions: &[String; 4],
    upstream: &str,
    accepted: &[usize; 4],
    blessed: bool,
) -> String {
    let mut out = String::from("### socat\n\n");
    let raced: Vec<String> = NAMES
        .iter()
        .zip(versions)
        .map(|(n, v)| format!("{n} {v}"))
        .collect();
    let _ = writeln!(out, "Raced: {}.", raced.join(" · "));
    if !upstream.is_empty() && !versions[0].starts_with(upstream) {
        let _ = writeln!(
            out,
            "\n**socat lag:** upstream lists {upstream}, the raced nixpkgs socat is {}.",
            versions[0]
        );
    }
    let mut all = Tally::default();
    let mut origins: BTreeMap<String, Tally> = BTreeMap::new();
    let mut types: BTreeMap<String, Tally> = BTreeMap::new();
    let mut options: BTreeMap<String, Tally> = BTreeMap::new();
    for row in matrix.rows.values() {
        all.add(row.class);
        let mut parts = row.family.splitn(3, ':');
        let origin = parts.next().unwrap_or("");
        let kind = parts.next().unwrap_or("");
        let family = parts.next().unwrap_or("").to_string();
        origins
            .entry(format!("{origin}:{kind}"))
            .or_default()
            .add(row.class);
        let table = match (origin, kind) {
            ("socat", "type") => &mut types,
            ("socat", "option") => &mut options,
            _ => continue,
        };
        let t = table.entry(family).or_default();
        t.add(row.class);
        if row.class == Class::Supported && typed(&row.entry) {
            t.typed += 1;
        }
    }
    let _ = writeln!(
        out,
        "\nMatrix: {} rows — {} supported · {} alternative · {} rejected.",
        all.total(),
        all.supported,
        all.alternative,
        all.rejected
    );
    let accepts: Vec<String> = NAMES
        .iter()
        .zip(accepted)
        .map(|(n, a)| format!("{n} {a}"))
        .collect();
    let _ =
        writeln!(out, "Entries each oracle accepts: {}.", accepts.join(" · "));
    match (misses.len(), blessed) {
        (0, _) => out.push_str("\n**Result: every row matches the matrix.**\n"),
        (n, true) => {
            let _ = writeln!(out, "\n**Blessed:** {n} mismatches were rewritten into the matrix; review its diff.");
        }
        (n, false) => {
            let _ = writeln!(out, "\n**Result: {n} mismatches with the matrix** (`crates/socat/peers/matrix.tsv`):\n");
            for m in misses.iter().take(200) {
                let _ = writeln!(
                    out,
                    "- `{}` ({}): {}",
                    crate::tsv::escape(&m.entry),
                    m.family,
                    m.reason
                );
            }
            if misses.len() > 200 {
                let _ = writeln!(out, "- … and {} more", misses.len() - 200);
            }
        }
    }
    out.push_str("\n| corpus | rows | supported | alternative | rejected |\n| --- | --- | --- | --- | --- |\n");
    for (k, t) in &origins {
        let _ = writeln!(
            out,
            "| {k} | {} | {} | {} | {} |",
            t.total(),
            t.supported,
            t.alternative,
            t.rejected
        );
    }
    out.push_str("\nCoverage of socat's address types, per family (supported or alternative / all):\n\n| family | types | supported | alternative | rejected |\n| --- | --- | --- | --- | --- |\n");
    for (k, t) in &types {
        let _ = writeln!(
            out,
            "| {k} | {} | {} | {} | {} |",
            t.total(),
            t.supported,
            t.alternative,
            t.rejected
        );
    }
    out.push_str("\nsocat's options, per option group. Each rides on a type we parse where one fits; a parsed option is *typed* (`mux`/`fork`/`max-children`) or carried *unchecked* to the backend (#59/#60):\n\n| group | options | typed | unchecked | alternative | rejected |\n| --- | --- | --- | --- | --- | --- |\n");
    for (k, t) in &options {
        let _ = writeln!(
            out,
            "| {k} | {} | {} | {} | {} | {} |",
            t.total(),
            t.typed,
            t.supported - t.typed,
            t.alternative,
            t.rejected
        );
    }
    out
}
