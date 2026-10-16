# crates/socat

Parses socat-style address strings into typed `Address` values and prints them back in one canonical spelling (#57, PR #67). It's a pure crate: no sockets and no name resolution. #59 (opening addresses, the bridge), #60 (WS/WSS/WebTransport) and #50 (the CLI) consume it.

The README's L1 section is the user-facing contract for the supported subset. This file records the research and decisions behind it, so the next change doesn't re-derive them.

## Settled decisions (don't re-litigate)

- **Canonical spellings:** `TCP-CONNECT:`, `UDP-CONNECT:`, `*-LISTEN:`. Bare `TCP:`, `UDP:`, `UNIX:`, `LOCAL:` parse but print as `-CONNECT`.
- **`PTY` stays.** The README and #59/#60 decide which L1s exist. Examples are not contracts.
- **`,fork` is a listen-only alias of `,mux`**, not socat's process-fork option. socat's own semantics (`fork` on connect, `fork=0`/`fork=2`) are deliberately not modelled. `max-children` requires `mux`.
- **No Windows paths.** Backslash escapes follow socat on POSIX.
- **Which addresses can mux, and which need a peer table,** is #59's **Channels** section. This crate only has to let #59 tell single-stream types (`STDIO`, `FILE`/`OPEN`/`GOPEN`, `PTY`) from the rest (`Target::mode()` is `None` for them, and for `EXEC`).

## Coverage

L1 transports the README names, and how each is reached:

| README transport | Address | Notes |
| --- | --- | --- |
| tcp, udp | `TCP`/`UDP` `-CONNECT`/`-LISTEN` | Host: name (UTF-8 allowed), IPv4, `[IPv6%zone]`. Port: `Port::Number` (decimal, leading `+`) or `Port::Service` (resolved by the backend). |
| unix | `UNIX`/`ABSTRACT` `-CONNECT`/`-LISTEN` | `ABSTRACT` is a distinct type; a leading `@` in a `UNIX` path is literal. |
| vsock | `VSOCK-CONNECT:cid:port`, `VSOCK-LISTEN:port` | `u32` fields, decimal only. |
| serial | `FILE`/`OPEN`/`GOPEN:/dev/ttyS0,b115200,raw`, or a bare path (= `GOPEN`) | **Serial options (`b115200`, `raw`, `echo=0`) are untyped**: they ride in `rest`. |
| pty, stdio, exec | `PTY`, `STDIO`/`-`, `EXEC:cmd` | PTY options (`link=`, `raw`, …) are untyped too. |
| ws, wss, webtransport | `WS`/`WSS`/`WEBTRANSPORT` `-CONNECT`/`-LISTEN` with an optional `Resource` path; `ws://`, `wss://`, `https://` URLs | URLs are connect-only. |

Every direction type also takes websocat's short forms (`TCP-L`, `L-TCP`, `LISTEN-TCP`, `C-TCP`, …).

**Options:** only `mux`/`fork` and `max-children` are typed. Everything else is `rest: Unchecked`: parsed and printed, never validated. Near-miss spellings of typed names (`max_children`, `MaxChildren`) are normalized so they can't silently become no-ops (tocat's documented bug class). **The consumer that opens an address (#59/#60) must reject `rest` entries it doesn't implement**, or `TCP-LISTEN:80,bogus=1` is silently accepted.

**Rejected with a positioned error, deliberately:**
- `TCP4`/`TCP6`/`UDP4`/`UDP6` family pins;
- socat's datagram modes (`UDP-SENDTO`/`-RECVFROM`/`-RECV`/`-DATAGRAM`, `UNIX-SENDTO`/`-RECVFROM`/`-CLIENT`);
- `FD` and the numeric-FD shorthand;
- `STDIN`/`STDOUT`/`STDERR`, `SYSTEM`/`SHELL`, `PIPE`, `CREATE`;
- `OPENSSL*`, `SOCKS*`, `PROXY`: secure and proxied pipes compose externally via `EXEC:` (README *Crypto*);
- `SCTP`, `DCCP`, `UDPLITE`, `IP-*`, `TUN`, `INTERFACE`, `READLINE`, `POSIXMQ`;
- `!!` dual addresses, which are rejected unquoted, and the printer quotes any literal `!!`;
- base-0 numbers (`010`, `0x50`) for VSOCK ids and `max-children`.

Adding one of these means a deliberate spec change (README L1 plus a #59/#60 consumer), not a parser tweak.

## socat behaviour confirmed by probing socat 1.8.0.0

Use these when a reference question comes up. They were run locally against the real binary, and they override clones that disagree.

- Named escapes `\n \r \t \f \b \a` are converted outside single quotes; an unknown escape drops the backslash (`\q` → `q`). **`\xHH` is not an escape** (`\x41` → `x41`): oittaa/socat's `\xHH` is a divergence.
- Single quotes are verbatim; double quotes process escapes.
- A stray closer with nothing open (`EXEC:a]b`) is literal. A mismatch inside an open group (`EXEC:(a]`) is an error.
- Empty option fields are skipped (`TCP:h:80,`, `TCP:h:80,,reuseaddr`), but an empty numeric value (`fork=`) is an error.
- `[fe80::1%lo]` zones and IDN hosts (`bücher.de`) pass through to `getaddrinfo`.
- TCP/UDP ports accept service names and `+80`; `0x50` is a service name, not hex. `max-children` is a signed C `int` with base 0 (we reject base 0, see above).
- GOPEN is inferred only when `/` comes before the first `:` or `,`.
- `OPEN` (`open(2)`) and `GOPEN` (also connects to sockets) differ, so they're distinct variants.

## Prior art

The four reports are in the session scratchpad (Claude and k3 sweeps, 2026-10-15). The sources worth re-reading:

| Source | License | What it's good for |
| --- | --- | --- |
| socat 1.8 manual, `doc/xio.help` | GPL-2.0, **docs only, never code** | the reference grammar |
| [oittaa/socat](https://github.com/oittaa/socat) (Go clone) | MIT | the most complete reimplementation: scanner, dual split, option metadata, zones. Diverges on `\xHH`, English booleans, base-0 ports. |
| [vi/websocat](https://github.com/vi/websocat) `specparse.rs`, `doc.md` | MIT | direction prefixes, `ws://` URLs |
| [lkalir/tocat](https://github.com/lkalir/tocat) `endpoint/parse.rs`, `layer/ws.rs`, `normalize.rs` | MIT OR Apache-2.0 | per-scheme option rejection, keyword normalization, WS path default `/` |
| Twisted `endpoints.py` (`quoteStringArgument`) | MIT | printer quoting as the inverse of the lexer |
| Go `net.SplitHostPort` | BSD-3-Clause | bracketed host and zone splitting |
| multiaddr (go/rust/py) | MIT / Apache-2.0 | `ip6zone`, IDN storage |

No source expression was imported, only behaviour and types (clean-room). No patent flags.

## Tests

- `src/address/test.rs` holds the contract tests. `proptest-regressions/address/test.txt` is proptest's own seed file: it saves the shrunk inputs of past failures and replays them before new cases. Commit it, and don't edit it by hand.
- Round 1 showed that the round-trip property (`check`: parse → print → parse) **can't catch a parse that drops data**, because it reparses the printer's own output. Six targeted mutants survived the suite until exact assertions were added. Keep the suite mutant-killing:
  - typed or canonical assertions for bare paths (`/a:b`);
  - extra-parameter rejection in **both** directions of every fixed-arity parser;
  - `0`, `1`, `u32::MAX` and `u32::MAX + 1` boundaries for VSOCK and `max-children`;
  - exact rejections for each deferred family;
  - exact byte offsets on non-ASCII input;
  - strategies that include empty values, control characters and non-ASCII text.
