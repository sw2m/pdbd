def round2: . * 100 | round / 100;
def cell($name; $tags; $unit):
  [.rows[] | select(.name == $name and .stats != null) | . as $row
    | select($tags | to_entries | all(.value as $v | $row.tags[.key] == $v))]
  | if length == 0 or .[0].stats[$unit] == null then "—"
    else .[0].stats[$unit] as $s
      | ($s.median | round2 | tostring)
        + (if $s.ci then " (\($s.ci[0] | round2)–\($s.ci[1] | round2))" else "" end)
    end;
def verdict($name; $impl):
  [.rows[] | select(.name == $name and .tags.impl == $impl)] | first // null | if . == null then "—" else .verdict end;
. as $report
| [.rows[] | select(.name == "comparator") | .tags] as $impls
| ([.rows[] | select(.name == "bulk") | .stats["MiB/s"].n] | max // 0) as $repeat
| "### mux",
  "",
  ("Every impl races in its own language over one loopback TCP pair, in a bencher container (its image from bencher's "
   + "\($report.cache.level // "unknown") cache); see pdbd-bench's mux/README.md for each scenario, the fairness settings and the "
   + "statistics. Each cell is the median of \($repeat) repetitions, with its 95% confidence interval when there are enough. "
   + "Ours is `core` (two sessions fed into each other, no sockets; compare it with `nghttp2-core`) and `loopback` "
   + "(a single-threaded busy-poll loop: a lower bound for #59's event loop), linked statically against glibc like the native comparators: "
   + "on musl's allocator, its per-frame allocations ran 14× slower (#83). An HTTP/2 flow is a POST, so opening one sends headers."),
  "",
  ("Raced: " + ([$impls[] | if .version then "\(.impl) \(.version) (\(.mode), \(.toolchain))" else "\(.impl) (budget \(.budget) B)" end] | join("; ")) + "."),
  "",
  "| impl | bulk MiB/s | 16 flows MiB/s | 256 flows MiB/s | Jain 256 | slowest 256 | bulk CPU ms/MiB |",
  "| --- | --: | --: | --: | --: | --: | --: |",
  ($impls[] | .impl as $i
    | "| \($i) | \($report | cell("bulk"; {impl: $i}; "MiB/s")) | \($report | cell("flows"; {impl: $i, flows: "16"}; "MiB/s")) "
      + "| \($report | cell("flows"; {impl: $i, flows: "256"}; "MiB/s")) | \($report | cell("flows"; {impl: $i, flows: "256"}; "jain")) "
      + "| \($report | cell("flows"; {impl: $i, flows: "256"}; "slowest")) | \($report | cell("bulk"; {impl: $i}; "ms/MiB")) |"),
  "",
  "| impl | isolated | stalled ÷ calm | ping p50 µs | ping p99 µs | loaded p50 µs | loaded p99 µs | loaded p99.9 µs |",
  "| --- | :-: | --: | --: | --: | --: | --: | --: |",
  ($impls[] | .impl as $i
    | "| \($i) | \($report | verdict("isolated"; $i)) | \($report | cell("isolation"; {impl: $i}; "ratio")) "
      + "| \($report | cell("pingpong"; {impl: $i}; "p50 µs")) | \($report | cell("pingpong"; {impl: $i}; "p99 µs")) "
      + "| \($report | cell("loaded"; {impl: $i}; "p50 µs")) | \($report | cell("loaded"; {impl: $i}; "p99 µs")) "
      + "| \($report | cell("loaded"; {impl: $i}; "p999 µs")) |"),
  "",
  "| impl | open/close per s | open p99 µs | idle B/flow |",
  "| --- | --: | --: | --: |",
  ($impls[] | .impl as $i
    | "| \($i) | \($report | cell("churn"; {impl: $i}; "cycles/s")) | \($report | cell("churn"; {impl: $i}; "open p99 µs")) "
      + "| \($report | cell("idle"; {impl: $i}; "B/flow")) |"),
  "",
  "| UDP datagram | core datagrams/s | loopback datagrams/s |",
  "| --: | --: | --: |",
  ([.rows[] | select(.name == "udp") | .tags.datagram] | unique | sort_by(tonumber)[] as $d
    | "| \($d) B | \($report | cell("udp"; {impl: "core", datagram: $d}; "datagrams/s")) | \($report | cell("udp"; {impl: "loopback", datagram: $d}; "datagrams/s")) |"),
  (if ($report.errors | length) > 0 then "", "Errors: " + ([$report.errors[] | "\(.stage): \(.message)"] | join("; ")) else empty end)
