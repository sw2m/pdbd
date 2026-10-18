def round2: . * 100 | round / 100;
def cell($name; $tags; $unit):
  [.rows[] | select(.name == $name and (.tags | contains($tags)))]
  | if length == 0 or .[0].stats == null then ""
    else .[0].stats[$unit] as $s
      | ($s.median | round2 | tostring)
        + (if $s.ci then " (\($s.ci[0] | round2)–\($s.ci[1] | round2))" else "" end)
    end;
. as $report
| ([.rows[] | select(.name == "link")]) as $links
| ([.rows[] | .tags.pppd // empty] | unique | join(", ") | if . == "" then "(none ran)" else . end) as $pppd
| ([.rows[] | select(.name == "bring-up") | .stats.ms.n] | max // 0) as $repeat
| "### transport",
  "",
  ("L1: a socat PTY pair in a bencher VM (its image from bencher's \($report.cache.level // "unknown") cache); L2: pppd \($pppd), "
   + "linkup's static musl build. Pairings are near ↔ far: near runs in its own netns and starts once far's interface exists, "
   + "→ is near to far. Each cell is the median of \($repeat) runs, with its 95% confidence interval when there are enough samples; "
   + "RTT pools every ping reply. CPU is busy time on the benchmark's own CPUs (bencher fences it there, so this counts kernel PPP work) "
   + "per MB received over the three iperf3 runs. pppd ↔ pppd bring-up includes pppd's 3 s restart timer: its first ConfReq is lost to the PTY flush."),
  "",
  "| pairing | bring-up ms | → Mbit/s | ← Mbit/s | → ×4 Mbit/s | RTT idle ms | RTT loaded ms | CPU ms/MB |",
  "| --- | --: | --: | --: | --: | --: | --: | --: |",
  ($links[] | .tags.pairing as $p | {pairing: $p} as $t
    | "| \($p | split(".") | join(" ↔ "))\(if .verdict == "err" then " (some runs failed)" else "" end) "
      + "| \($report | cell("bring-up"; $t; "ms")) "
      + "| \($report | cell("throughput"; $t + {direction: "forward"}; "Mbit/s")) "
      + "| \($report | cell("throughput"; $t + {direction: "reverse"}; "Mbit/s")) "
      + "| \($report | cell("throughput"; $t + {direction: "parallel"}; "Mbit/s")) "
      + "| \($report | cell("rtt"; $t + {load: "idle"}; "ms")) "
      + "| \($report | cell("rtt"; $t + {load: "loaded"}; "ms")) "
      + "| \($report | cell("cpu"; $t; "ms/MB")) |"),
  (if ($report.errors | length) > 0 then "", "Errors: " + ([$report.errors[] | "\(.stage): \(.message)"] | join("; ")) else empty end)
