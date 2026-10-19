def class($ours; $alternatives): if $ours[.] == "ok" then "supported" elif $alternatives[.] != null then "alternative" else "rejected" end;
def counts: group_by(.class) | map({key: .[0].class, value: length}) | from_entries;
def cells: "\(.supported // 0) | \(.alternative // 0) | \(.rejected // 0)";
. as $report
| ([.rows[] | select(.name == "parse" and .tags.impl == "ours") | {key: .tags.input, value: .verdict}] | from_entries) as $ours
| ([.rows[] | select(.name == "alternative") | {key: .tags.input, value: .verdict}] | from_entries) as $alternatives
| [.rows[] | select(.name == "corpus") | .tags | . + {class: (.input | class($ours; $alternatives))}] as $corpus
| [.rows[] | select(.name == "oracle") | .tags] as $oracles
| [.expectations[] | select(.pass | not)] as $missed
| "### socat",
  "",
  ("Raced: " + ([$oracles[] | "\(.impl) \(.version) (\(.mode))"] | join(" · ")) + "."),
  "",
  ("Corpus: \($corpus | length) entries — " + ($corpus | counts | "\(.supported // 0) supported · \(.alternative // 0) alternative · \(.rejected // 0) rejected") + "."),
  "",
  (if ($report.errors | length) > 0 then "**Errors:** " + ([$report.errors[] | "\(.stage): \(.message)"] | join("; "))
   elif ($missed | length) == 0 then "**Every row matches `expect.tsv`.**"
   else "**\($missed | length) rule(s) missed** — decide each, then `bencher bless` (crates/socat/CLAUDE.md §\"Benchmark\"):",
     "",
     ($missed[:50][] | "- line \(.line): `\(.rule | gsub("\t"; " "))`" + (if .error then " — \(.error)" else "" end)
       + ([.checks[] | select(.pass | not) | " — \(.tags | tojson) is \(.actual | tojson)"] | .[:3] | join(""))),
     (if ($missed | length) > 50 then "- … and \(($missed | length) - 50) more" else empty end)
   end),
  "",
  "| corpus | entries | supported | alternative | rejected |",
  "| --- | --: | --: | --: | --: |",
  ($corpus | group_by(.family | split(":")[:2] | join(":"))[]
    | "| \(.[0].family | split(":")[:2] | join(":")) | \(length) | \(counts | cells) |"),
  "",
  "socat's address types, per family:",
  "",
  "| family | types | supported | alternative | rejected |",
  "| --- | --: | --: | --: | --: |",
  ([$corpus[] | select(.family | startswith("socat:type:"))] | group_by(.family)[]
    | "| \(.[0].family | ltrimstr("socat:type:")) | \(length) | \(counts | cells) |"),
  "",
  "socat's options, per option group (each riding a type whose groups fit):",
  "",
  "| group | options | supported | alternative | rejected |",
  "| --- | --: | --: | --: | --: |",
  ([$corpus[] | select(.family | startswith("socat:option:"))] | group_by(.family)[]
    | "| \(.[0].family | ltrimstr("socat:option:")) | \(length) | \(counts | cells) |")
