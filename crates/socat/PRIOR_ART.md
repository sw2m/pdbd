# Prior art

What crate `socat`'s address dialect (#57; README §"L1 — the transport") was built against, and
what its benchmark races (README §"Benchmarks"). Everything here was reimplemented from
documentation and observed behaviour, never from source: only behaviour and types were imported,
no source expression (clean-room), and no patent flags were raised.

## Raced by the benchmark

pdbd-bench's [`socat` runner](https://github.com/sw2m/pdbd-bench/blob/main/socat/README.md) asks
each of these whether every corpus entry parses, at its latest release unless
`params.pins.<oracle>` in `bench/bench.toml` pins it, and records each answer as a verdict row in
`bench/expect.tsv`.

| Oracle | Language · license | Asked through | Raced (2026-10-19) | Accepts, of 1,987 entries |
| --- | --- | --- | --- | --- |
| [socat](http://www.dest-unreach.org/socat/) | C · GPL-2.0 | `socat -d -d <entry> /dev/null`; a syntax error rejects, anything else accepts | 1.8.1.3 | 1,660 |
| [websocat](https://github.com/vi/websocat) | Rust · MIT | `websocat --dump-spec <entry>` | v1.14.1 | 161 |
| [oittaa/socat](https://github.com/oittaa/socat) | Go · MIT | its CLI, as for socat | v1.0.3 | 1,414 |
| [tocat](https://github.com/lkalir/tocat) (`tocat-core`) | Rust · MIT OR Apache-2.0 | `EndpointSpec::from_str` | 0.3.0 | 214 |

The corpus is generated from these same four: socat's and oittaa's `-hhh`, oittaa's parser tests,
websocat's `--help=full` and `doc.md`, and tocat's scheme table, plus `bench/hand.jsonl`.

### socat — the reference

The [socat(1)](http://www.dest-unreach.org/socat/doc/socat.html) manual and `doc/xio.help` are the
reference grammar. socat is GPL-2.0, so it is read for its documentation and probed for its
behaviour, and its code is never consulted.

Adopted: the address-spec grammar, case-insensitive keywords and options, the `-` synonym, the
bare-path `GOPEN` rule, the quote, bracket and escape rules, `!!` as the dual separator, `TCP`/
`UDP`/`UNIX`/`LOCAL` meaning connect, `OPEN` and `GOPEN` as distinct types, `ABSTRACT-*`,
service-name ports, `fork` and `max-children`.

Confirmed by probing socat 1.8.0.0, and overriding any reimplementation that disagrees:

- named escapes `\n \r \t \f \b \a \v` are converted outside single quotes; there is no octal
  (`\101` → `101`), an unknown escape drops its backslash (`\q` → `q`), and **`\xHH` is not an
  escape** (`\x41` → `x41`);
- `\0` becomes NUL, which truncates socat's C string and silently drops the rest; we reject it
  (`Kind::Nul`) rather than copy the data loss;
- single quotes are verbatim, double quotes process escapes;
- a stray closer with nothing open (`EXEC:a]b`) is literal, a mismatch inside an open group
  (`EXEC:(a]`) is an error;
- empty option fields are skipped (`TCP:h:80,`, `TCP:h:80,,reuseaddr`), an empty numeric value
  (`fork=`) is an error;
- `[fe80::1%lo]` zones and IDN hosts (`bücher.de`) pass through to `getaddrinfo`;
- TCP/UDP ports take service names and `+80`, and `0x50` is a service name, not hex;
  `max-children` is a signed C `int` read in base 0, which we reject (`Kind::Base`);
- `GOPEN` is inferred only when `/` comes before the first `:` or `,`;
- `OPEN` (`open(2)`) and `GOPEN` (which also connects to sockets) differ.

Confirmed against 1.8.1.3 by the benchmark: `max-children` without `fork` is a syntax error
("not allowed without option fork"), as `max-children` without `mux` is for us. socat binds
before it checks that, so two LISTENs racing for one port must each get their own network
namespace, or the loser's bind failure reads as acceptance (#67, round 6).

### oittaa/socat

The most complete reimplementation (Go): its scanner, the dual split and a quoted-`!!` test,
skipped empty option fields, the literal stray closer, IPv6 zones, the `maxchildren` alias, named
escapes, a WSS positional path. Deliberate divergences, because real socat disagrees with it:
its `\xHH` decoding, its base-0 TCP ports and its English boolean words. The benchmark races its
CLI rather than its packages: v1.0.3, its latest release, lacks master's `xio.PrepareChannel`.

### websocat

`doc.md` and `specparse.rs` (Rust): the direction prefixes `X-L`/`L-X`/`LISTEN-X` and
`X-C`/`C-X`/`CONNECT-X`, and `ws://`/`wss://` URLs with paths, both adopted. Its overlays and most
of its address types aren't, so the benchmark records them as rejected or as an alternative.

### tocat

`endpoint/parse.rs`, `layer/ws.rs` and `normalize.rs` (Rust): per-scheme option rejection,
keyword normalization (case, `-` and `_`), and a WS path that defaults to `/`. Its
"`tcp:80,append` used to parse and then silently do nothing" bug is why option names that
normalize to a typed option (`max_children`, `Fo-Rk`) are that option, and can never ride
unchecked.

## Consulted, not raced

| Source | License | What it gave |
| --- | --- | --- |
| [jmsun0/pysocat](https://github.com/jmsun0/pysocat) (Python) | Apache-2.0 | `split_outside_quotes`; `ws`/`wss` URL handling |
| [littleli/wsocat](https://github.com/littleli/wsocat) (Go) | Apache-2.0 | a quote-preserving address parser |
| Twisted `endpoints.py` (`quoteStringArgument`) | MIT | printer quoting as the inverse of the lexer |
| Go `net.SplitHostPort`, RFC 3986 §3.2.2 | BSD-3-Clause | bracketed hosts and `%zone` splitting |
| [multiaddr](https://github.com/multiformats/multiaddr) (Go, JS, Rust, Python) | MIT / Apache-2.0 | typed components with a normalizing printer (string → typed → canonical string); `ip6zone` as its own component |
