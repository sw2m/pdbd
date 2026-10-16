# crates/socat

Parses socat-style address strings into typed `Address` values and prints them back in one canonical spelling (#57, PR #67). It's a pure crate: no sockets and no name resolution. #59 (opening addresses, the bridge), #60 (WS/WSS/WebTransport) and #50 (the CLI) consume it.

The README's L1 section is the user-facing contract for the supported subset. This file records the research and decisions behind it, so the next change doesn't re-derive them.

## Settled decisions (don't re-litigate)

- **Canonical spellings:** `TCP-CONNECT:`, `UDP-CONNECT:`, `*-LISTEN:`. Bare `TCP:`, `UDP:`, `UNIX:`, `LOCAL:` parse but print as `-CONNECT`.
- **`PTY` stays.** The README and #59/#60 decide which L1s exist. Examples are not contracts.
- **`,fork` is a listen-only alias of `,mux`**, not socat's process-fork option. socat's own semantics (`fork` on connect, `fork=0`/`fork=2`) are deliberately not modelled. `max-children` requires `mux`.
- **No Windows paths.** Backslash escapes follow socat on POSIX.
- **Which addresses can mux, and which need a peer table,** is #59's **Channels** section. This crate's half is **`Target::single()`**, the one classification #59 keys on: it is `true` for the single-stream types (`STDIO`, `FILE`/`OPEN`/`GOPEN`, `PTY`) and `false` for everything openable per flow (sockets and `EXEC`). Don't use `Target::mode()` for this, because it is `None` for `EXEC` too. A new variant must pick a side in `single()` and in its test.

## Coverage

L1 transports the README names, and how each is reached:

| README transport | Address | Notes |
| --- | --- | --- |
| tcp, udp | `TCP`/`UDP` `-CONNECT`/`-LISTEN` | Host: name (UTF-8 allowed), IPv4, `[IPv6%zone]`. Port: `Port::Number` (decimal, leading `+`) or `Port::Service` (resolved by the backend). |
| unix | `UNIX`/`ABSTRACT` `-CONNECT`/`-LISTEN` | `ABSTRACT` is a distinct type; a leading `@` in a `UNIX` path is literal. |
| vsock | `VSOCK-CONNECT:cid:port`, `VSOCK-LISTEN:port` | `u32` fields, decimal only. |
| serial | `FILE`/`OPEN`/`GOPEN:/dev/ttyS0,b115200,raw`, or a bare path (= `GOPEN`) | **Serial options (`b115200`, `raw`, `echo=0`) are untyped**: they ride in `rest`. |
| pty, stdio, exec | `PTY`, `STDIO`/`-`, `EXEC:cmd` | PTY options (`link=`, `raw`, …) are untyped too. |
| ws, wss, webtransport | `WS`/`WSS`/`WEBTRANSPORT` `-CONNECT`/`-LISTEN` with an optional `Resource` path; `ws://`, `wss://`, `https://` URLs | URLs are connect-only. A query-only URL (`ws://h?x`) gets the path `/?x`. `#` fragments are rejected, because a fragment is never sent in the handshake. A host-less listen whose path has a colon must quote it (`WS-LISTEN:8080'/a:b'`). A slashed first listen token followed by more parameters (`WS-LISTEN:'h/x':80`) is rejected as `Kind::Port`, never reread as service plus path. |

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
- base-0 numbers (`010`, `0x50`, and any `0`-led alphanumeric such as `0b1` or `00x1`) for VSOCK ids and `max-children`, as `Kind::Base`;
- the `\0` escape (`Kind::Nul`). socat truncates its C string there, silently dropping the rest; we refuse it rather than copy the data loss;
- an empty positional parameter (`TCP:h:80:`, `STDIO:`), as `Kind::Blank`. Empty *option* fields are skipped instead.

Adding one of these means a deliberate spec change (README L1 plus a #59/#60 consumer), not a parser tweak.

## socat behaviour confirmed by probing socat 1.8.0.0

Use these when a reference question comes up. They were run locally against the real binary, and they override clones that disagree.

- Named escapes `\n \r \t \f \b \a \v` are converted outside single quotes. `\0` becomes NUL, which truncates the C string; we reject it as `Kind::Nul`, a deliberate divergence. There is no octal (`\101` → `101`). an unknown escape drops the backslash (`\q` → `q`). **`\xHH` is not an escape** (`\x41` → `x41`): oittaa/socat's `\xHH` is a divergence.
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

## Benchmark

**A differential race against the prior-art parsers** (#57; README §"Benchmarks"). Each oracle is asked whether a string parses, we try to parse the same string, and the answers are compared with a committed expected-results matrix. The matrix is this benchmark's published expectation, so any difference fails the `benchmark (socat)` check, and a developer decides what it means.

**Running it.** `LEFTHOOK_CONFIG=crates/socat/lefthook.yml lefthook run benchmark` (GitHub CI runs it through the fanout). The hook runs `peers/compare.sh` inside `nix shell` of an **unlocked** `nixpkgs-unstable` (go, cargo, rustc, gcc, curl, cacert, util-linux), the same mechanism as `crates/mux`. Nothing it resolves touches the root `Cargo.lock`, the root `flake.lock` or any `output` hash. `peers/rust` is its own Cargo workspace, and its `Cargo.lock` and `target/` are gitignored. `compare.sh`:

1. resolves and fetches each oracle (pin files below) and prints the versions it raced;
2. builds `peers/rust`, the harness that holds the corpus, the oracle runners, the check and the report; it depends on this crate by path and on `tocat-core`;
3. runs the harness inside an **empty network namespace** (`unshare -rn`, falling back to `sudo -n unshare -n` with `setpriv` back to the caller), so no oracle can reach a network.

**The corpus is generated from the prior art at run time.** A new oracle release therefore brings its new forms with it.

| Source | Entries |
| --- | --- |
| socat `-hhh` | every address type and alias, as a minimal valid form (`<host>` → `127.0.0.1`, `<port>` → `1`, `<filename>` → `f`, …); every option and alias, with a sample value by its `type=`, riding on the first type whose `groups=` fit. That host type is one we parse where possible, and a LISTEN type for `fork`/`max-children`. |
| oittaa/socat `-hhh` | its own type list (aliases take their target's signature), and its options riding on a host chosen by their help heading |
| oittaa/socat tests | every string literal passed to `ParseChannel`/`ParseSpec` in `internal/parse/*_test.go` (`peers/go`, a `go/ast` walk) |
| websocat `--help=full` + its `doc.md` | every address type and overlay, using a doc example where one exists; every doc alias; every doc example specifier |
| tocat | every scheme in `EndpointSpec::from_str`'s `match normalize(scheme)` table, scraped from the resolved crate's source, with a sample body |
| `peers/hand.txt` | lexer edges, the #67 review probes, the README's forms, and forms that wrapper tools generate (socatui, testcontainers, SocatTUI). Lines are escaped as in the matrix (`\\`, `\t`, `\u{b}`). |

**The oracles**, each asked without opening anything real:

- **socat** (from nixpkgs) and **oittaa/socat** (its CLI, `go install …/cmd/socat`) run as `-d -d <entry> /dev/null` with `PATH=/nonexistent`, in a scratch cwd. A **syntax** error is a rejection: unknown address, wrong parameter count, unknown or inapplicable option, missing numeric value, `not allowed without`, `parse left address`. Everything else counts as acceptance: a runtime failure (unreachable, ENOENT, `execvp` failing on the empty `PATH`), exit 0, or still running after 400 ms (a LISTEN blocked in accept). oittaa mirrors socat's messages, so one classifier serves both. oittaa's latest release (v1.0.3) has none of master's `xio.PrepareChannel`, which is why it is raced through its CLI and not its internal packages.
- **websocat** (the GitHub release binary): `-t -E --dump-spec <entry> -`, where exit 0 is acceptance. It resolves host names while building the spec, and in the empty namespace that blocks until its resolver times out, so a dump still running after 3 s has parsed the spec and counts as accepted.
- **tocat**: `entry.parse::<tocat_core::endpoint::EndpointSpec>()`, in process. A panic counts as rejection.

**The matrix** (`peers/matrix.tsv`, sorted by family and then entry) has one row per corpus entry: `entry · family · socat · websocat · oittaa · tocat · class · ours`.
- Each oracle cell is `ok@<version>` or `err@<version>`: the verdict and the release that gave it.
- The class is one of:
  - `supported`: we parse it, and `ours` is our canonical print;
  - `rejected`: a deliberate rejection (README §L1, and the deliberate list under §"Coverage" above), with `ours` set to `-`;
  - `alternative`: we reject it, and `ours` is the form we accept instead. The check confirms that form still parses.
- Families are `<origin>:<kind>[:<family>]`, e.g. `socat:type:TCP`, `socat:option:SOCKET`, `websocat:overlay`, `hand:lexer`.

**The check fails** (exit 1) when an entry is new, when a row is no longer in the corpus, when an oracle's verdict differs from the recorded one (only the verdict is compared, not the version), or when our result contradicts the class. Each mismatched row is named in a `### socat` section appended to `"${BENCHMARK_REPORT:-/dev/stdout}"`, with the oracle version. That section also reports the versions raced, the upstream socat when nixpkgs lags it, the class counts per corpus, and coverage. Coverage is socat's address types per family, and socat's options per group as *typed* (`mux`/`fork`/`max-children`), *unchecked* (carried in `rest` for #59/#60) or rejected. The coverage numbers only report.

**Resolving a failure is a developer decision.**
1. Run with `BENCHMARK_BLESS=1`. That rewrites the matrix from the run. Rows that still match keep their class; new or contradicted rows get a default class: `supported` if we parse it, otherwise `alternative` when `matrix.rs`'s `alternative()` maps it onto a form we parse (for example `TCP4` → `TCP`, tocat's `tcplisten` → `TCP-LISTEN`, `SYSTEM:cmd` → `EXEC:'sh -c …'`, `OPENSSL:h:p` → `EXEC:'openssl s_client …'`), otherwise `rejected`.
2. Read the diff. Decide each changed row: support it (a spec change: README L1 plus a crate change), reject it deliberately, or record an alternative.
3. Edit the class and the `ours` field, and commit the matrix with the reason in the commit message.

**The "stop racing" switch.** Every oracle floats by default. Pinning one is deliberate and recorded in exactly one place:

| Oracle | Pin here |
| --- | --- |
| socat | `peers/socat/nixpkgs`: replace `github:NixOS/nixpkgs/nixpkgs-unstable` with a nixpkgs commit. socat comes from nixpkgs because upstream's downloads are plain http without signatures (its https certificate is self-signed). |
| websocat | `peers/websocat/version`: replace `latest` with a release tag |
| oittaa/socat | `peers/go/versions`: replace `@latest` with `@vX.Y.Z` |
| tocat | `peers/rust/Cargo.toml`: `tocat-core = "*"` → `"=x.y.z"` |
| toolchains | `crates/socat/lefthook.yml`: replace `nixpkgs-unstable` with a commit |

Record why next to the pin, and in this file.

**Caveats.**
- An oracle verdict means "accepted its syntax", inferred from errors. It never proves the string would work.
- socat's lexer runs an unterminated quote into its next argument (`EXEC:'abc` executes `abc/dev/null`), so some of socat's acceptances of malformed input are artefacts of that.
- `EXEC`/`SYSTEM` entries do run, but with an empty `PATH` inside the empty namespace.

**Gaps the first run surfaced** (listed, not implemented; each needs a spec decision):
- bare `VSOCK` (socat 1.8.1.3's alias for `VSOCK-CONNECT`, like `UNIX`/`LOCAL`): recorded as `alternative` → `VSOCK-CONNECT`;
- socat 1.8.1's `TEXT:`, `STALL`, `ECHO`, `SOCKETPAIR` test types: `rejected`, but not yet in the deliberate list above.

## Tests

- `src/address/test.rs` holds the contract tests. `proptest-regressions/address/test.txt` is proptest's own seed file: it saves the shrunk inputs of past failures and replays them before new cases. Commit it, and don't edit it by hand.
- Round 1 showed that the round-trip property (`check`: parse → print → parse) **can't catch a parse that drops data**, because it reparses the printer's own output. Six targeted mutants survived the suite until exact assertions were added. Keep the suite mutant-killing:
  - typed or canonical assertions for bare paths (`/a:b`);
  - extra-parameter rejection in **both** directions of every fixed-arity parser;
  - `0`, `1`, `u32::MAX` and `u32::MAX + 1` boundaries for VSOCK and `max-children`;
  - exact rejections for each deferred family;
  - exact byte offsets on non-ASCII input;
  - strategies that include empty values, control characters and non-ASCII text.
