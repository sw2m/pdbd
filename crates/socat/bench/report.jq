# see README.md §"Benchmarks" and sw2m/pdbd#57 "Benchmark classes"
def class($ours; $alternatives; $classes):
  if $ours[.] == "ok" then "supported"
  elif $alternatives[.] != null then "alternative-" + $alternatives[.]
  else ($classes[.] // "invalid" | sub("^excluded-.*"; "excluded")) end;
def counts: group_by(.class) | map({key: .[0].class, value: length}) | from_entries;
def alternatives: ((."alternative-full" // 0) + (."alternative-partial" // 0));
def cells: "\(.supported // 0) | \(alternatives) | \(.excluded // 0) | \(.deferred // 0) | \(.invalid // 0)";
def header($what): "| \($what) | supported | alternative | excluded | deferred | invalid |", "| --- | --: | --: | --: | --: | --: | --: |";
. as $report
| ([.rows[] | select(.name == "parse" and .tags.impl == "ours") | {key: .tags.input, value: .verdict}] | from_entries) as $ours
| ([.rows[] | select(.name == "alternative" and .verdict == "ok") | {key: .tags.input, value: (.tags.fit // "full")}] | from_entries) as $alternatives
| ([.rows[] | select(.name == "class") | {key: .tags.input, value: .tags.class}] | from_entries) as $classes

| [.rows[] | select(.name == "corpus") | .tags | . + {class: (.input | class($ours; $alternatives; $classes))}] as $corpus
| ([$corpus[] | select(.class == "excluded" or .class == "deferred") | $classes[.input]] | group_by(.) | map({key: .[0], value: length}) | from_entries) as $why
| [.rows[] | select(.name == "oracle") | .tags] as $oracles
| [.expectations[] | select(.pass | not)] as $missed
| "### socat",
  "",
  ("Raced: " + ([$oracles[] | "\(.impl) \(.version) (\(.mode))"] | join(" · ")) + "."),
  "",
  ("Corpus: \($corpus | length) entries — " + ($corpus | counts | "\(.supported // 0) supported · \(alternatives) alternative (\(."alternative-full" // 0) full, \(."alternative-partial" // 0) partial) · \(.excluded // 0) excluded · \(.deferred // 0) deferred · \(.invalid // 0) invalid") + "."),
  "",
  ("Excluded by design: \($why."excluded-openssl" // 0) SSL-based (no universal L1; compose through `EXEC`), \($why."excluded-family" // 0) address-family pins. Deferred: \($why.deferred // 0) raw-Ethernet (#80). Invalid: input a standard or the grammar rejects."),
  "",
  (if ($report.errors | length) > 0 then "**Errors:** " + ([$report.errors[] | "\(.stage): \(.message)"] | join("; "))
   elif ($missed | length) == 0 then "**Every row matches `expect.tsv`.**"
   else "**\($missed | length) rule(s) missed** — decide each, then `bencher bless` (README.md §\"Benchmarks\"):",
     "",
     ($missed[:50][] | "- line \(.line): `\(.rule | gsub("\t"; " "))`" + (if .error then " — \(.error)" else "" end)
       + ([.checks[] | select(.pass | not) | " — \(.tags | tojson) is \(.actual | tojson)"] | .[:3] | join(""))),
     (if ($missed | length) > 50 then "- … and \(($missed | length) - 50) more" else empty end)
   end),
  "",
  header("corpus | entries"),
  ($corpus | group_by(.family | split(":")[:2] | join(":"))[]
    | "| \(.[0].family | split(":")[:2] | join(":")) | \(length) | \(counts | cells) |"),
  "",
  "socat's address types, per family:",
  "",
  header("family | types"),
  ([$corpus[] | select(.family | startswith("socat:type:"))] | group_by(.family)[]
    | "| \(.[0].family | ltrimstr("socat:type:")) | \(length) | \(counts | cells) |"),
  "",
  "socat's options, per option group (each riding a type whose groups fit):",
  "",
  header("group | options"),
  ([$corpus[] | select(.family | startswith("socat:option:"))] | group_by(.family)[]
    | (if all(.class == "excluded") then " (rides an excluded type)" else "" end) as $note
    | "| \(.[0].family | ltrimstr("socat:option:"))\($note) | \(length) | \(counts | cells) |")
