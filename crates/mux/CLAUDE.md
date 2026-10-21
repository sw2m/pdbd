# crates/mux

The bridge tunnel's multiplexer (#58, PR #68). It's a sans-IO state machine with a pull data path (#58 "Data path"): `ready(id)` marks a flow, `pull(out, source)` frames bytes the `Source` writes straight into the caller's buffer, which is the one copy of a payload byte; read into `spare` and `fill` it, `poll` events, and `held(id)` lends a flow's bytes from where they arrived until `consume`. `feed` is `spare` and `fill` plus a copy. #59 drives it over the one reliable TCP tunnel a bridge rides. The wire contract is the README's §"The bridge tunnel's wire contract". This file records how the design compares with the multiplexers we reviewed, so the next change reuses that research instead of redoing it.

- `Single`: one flow, no frame, the raw stream (flow id `0`).
- `Multi::new(side, window, cap, budget)`: framed flows. `cap` (max-children) and `budget` are required; the budget must hold one maximum frame (≥ 65535), or `new` returns `Error::Budget`. A 10-byte header (type, proto, id, length/credit). Types are `DATA 0`, `OPEN 1`, `ACCEPT 2`, `WINDOW 3`, `CLOSE 4`, `RESET 5`, `STOP 6`. Proto is `6` TCP or `17` UDP.

## Settled (don't re-litigate)

- **Liveness and drain are not wire features.** Liveness is L2 LCP echo; drain is the control plane's `Drop`. No yamux-style Ping/GoAway. That holds only while every bridge really has both paths.
- **No per-frame version byte.** `Hello.wire_version` covers versioning.
- **RESET codes:** only `REFUSED 0` and `ABORTED 1`, and any other code reads as `ABORTED`. A fuller reason table is deferred.
- **Which listeners mux and which need a peer table** is #59's **Channels** section. Mux is optional per listener; only `bind`/`forward` always mux.

## Settled: single-channel UDP

`Single` stays byte-transparent: one flow, no proto, no frame of its own. A single-channel UDP flow keeps its datagram boundaries because #59's worker frames each datagram itself (2-byte big-endian length, then the payload, never split), per #59's **Channels** section. Kimi k3's round-2 probe pushed u16-framed datagrams through `Single` in 7-byte chunks and every boundary survived. The mux's datagram-per-DATA-frame accounting and credit are multi-channel only.

## The multiplexers we compared

Swept 2026-10-15 (Claude and Kimi k3 osint passes), with a Rust pass on 2026-10-16. Licenses are noted because only ideas were imported, never code.

### Go

| Mux | License | Model | What we took or rejected |
| --- | --- | --- | --- |
| [hashicorp/yamux](https://github.com/hashicorp/yamux) | MPL-2.0 | 12-byte header, streams only; per-stream window 256 KiB; **no session window** (spec); SYN/ACK flags with **data allowed before ACK**; Ping, GoAway | Took the half-window replenish threshold, CLOSE as half-close and RESET as abort, and fatal-on-overrun. Rejected optimistic data-before-ACK: we want OPEN → ACCEPT/RESET so the far end can refuse before any bytes (SSH-style). Issues #133 (half-close credit stall), #43 (receive-buffer growth) and #50 (allocation per read) are warnings. |
| [xtaci/smux](https://github.com/xtaci/smux) | MIT | 8-byte header. v1 had only a **session-wide token bucket** (`MaxReceiveBuffer`); v2 added per-stream windows (`cmdUPD`) and kept both. Write shaper sends control frames first, then round-robin. | Took the **local receive budget** (`budget`, `paused()`): a memory bound that needs no wire change. Took control-first queueing and round-robin turns. Issues #37 (close/keepalive write deadlock) and #69 (heavy-load blocking, unresolved) are warnings. |
| [libp2p/go-yamux](https://github.com/libp2p/go-yamux) | MIT / Apache-2.0 | yamux fork with receive-window **auto-tuning** (double the window when updates arrive faster than every 2 RTTs, up to a max) | Took the ability, not the policy: `grow(id, window)` lets a caller enlarge a window by over-granting. Auto-tuning stays out of the crate. #136/#137 show the threshold must scale with the window. |
| [inconshreveable/muxado](https://github.com/inconshreveable/muxado) | NOASSERTION | per-stream windows | Read only; not used as a source. |
| [libp2p/go-mplex](https://github.com/libp2p/go-mplex) | MIT | **no flow control at all**; deprecated in favour of yamux/QUIC | The cautionary tale: a mux without per-stream credit lets one slow reader stall or bloat the session. |
| Go `x/net/http2` | BSD-3-Clause | stream **and** connection windows (`takeInflows`), chunked `dataBuffer`, priority/round-robin write scheduler | Defect history (#16481, #28204, #40423, #42983, #54185) is mostly window-update and scheduler stalls. Informs the control-first queue. |
| [quic-go](https://github.com/quic-go/quic-go) | MIT | stream + connection flow controllers with auto-tuning | #1545: an abandoned read must release credit (→ our `STOP`). #4843: a connection-level update can't wait on a stream-level one. |

### C# — Nerdbank.Streams `MultiplexingStream`

[dotnet/Nerdbank.Streams](https://github.com/dotnet/Nerdbank.Streams), MIT. The closest design to ours in intent: a general channel mux over one duplex stream, used by VS / vs-streamjsonrpc.

- Protocol v1 had no flow control. **v2 added per-channel windows** specifically so one full channel stops pausing the rest. v3 uses qualified channel ids (Local/Remote/Seeded) instead of parity.
- Offer/accept channel opening (`OfferChannelAsync` / `AcceptChannelAsync`), like our OPEN/ACCEPT.
- `ChannelWindowAdjust` (an absolute new size) and growth requests: our `grow`.
- **`ContentReadingCompleted`**: a receiver tells the sender it stopped reading, and the sender stops and frees its blocked writers. This is our `STOP` frame (type 6, `stop(id)`, `Event::Stopped`, `Error::Stopped`), the read-side counterpart of CLOSE.
- Replenish at window/8 with a floor: an optimization, not a correctness rule.

### Rust

| Crate | License | Model | Relevance |
| --- | --- | --- | --- |
| [libp2p/rust-yamux](https://github.com/libp2p/rust-yamux) (`yamux`) | Apache-2.0 | yamux wire; `max_num_streams` (512); **`max_connection_receive_window`** (default 1 GiB, must be ≥ 256 KiB × streams); per-stream windows **auto-tuned to the bandwidth-delay product** from a measured RTT (`connection/rtt.rs`) | The Rust confirmation that a session-level receive bound is needed on top of per-stream credit. Its docs name the memory-DoS reason. Same shape as our `cap` + `budget`, plus auto-tuning we left to callers via `grow`. Stream-only, so no datagrams. |
| [black-binary/async-smux](https://github.com/black-binary/async-smux) | MIT | smux v1/v2 over any tokio `AsyncRead + AsyncWrite`; connector/acceptor/worker split | Shows the I/O-owning shape we avoided: a spawned worker future drives the socket. We stay sans-IO, so #59 owns the tasks. |
| [hyperium/h2](https://github.com/hyperium/h2) | MIT | HTTP/2: `initial_window_size` + `initial_connection_window_size`; **explicit `release_capacity(n)`** after the app consumes data; `reserve_capacity` / `poll_capacity` on send; `send_reset`; caps on pending/reset streams | Our `consume(id, n)` is h2's `release_capacity`: credit returns only when the app has really consumed bytes, not on receipt. That avoids yamux #50's "assumed a read consumed the whole buffer". Its `max_pending_accept_reset_streams` / `max_concurrent_reset_streams` are the rapid-reset defence; we bound the same thing with `cap` and id retirement. |
| [quinn](https://github.com/quinn-rs/quinn) (`quinn-proto`) | Apache-2.0 / MIT | QUIC: `stream_receive_window`, connection `receive_window`, `send_window`, `max_concurrent_bidi_streams`, `max_idle_timeout`; **`RecvStream::stop(code)`** = STOP_SENDING; unreliable datagrams with `datagram_receive_buffer_size` | `stop` is the standard form of our STOP. QUIC datagrams carry no flow control because they're unreliable (RFC 9221 §5). Ours ride a reliable tunnel, so they **must** be credited, or UDP bypasses backpressure. |
| [russh](https://github.com/Eugeny/russh) | Apache-2.0 | SSH channels: `window_size` (2 MiB default), `maximum_packet_size` (32 KiB), per-channel `channel_buffer_size` | The SSH model (RFC 4254 §5) we follow most closely: credit granted in both OPEN and ACCEPT, consumed-byte window adjusts, a 32-bit window. |

### Datagrams over a stream

[mullvad/udp-over-tcp](https://github.com/mullvad/udp-over-tcp) (MIT / Apache-2.0): one `u16` big-endian length per datagram, never split, buffer compacted only after whole datagrams. This is the independent precedent for our `MAX = 65535` DATA bound (one datagram per frame), for `Blocked` when a datagram exceeds credit, for UDP windows ≥ 65535, and for the single-channel UDP frame above.

### Normative references

- SSH RFC 4254 §5.1–5.3: channels, windows, EOF vs close.
- HTTP/2 RFC 9113 §5.1.1 (id monotonicity, retiring skipped ids), §5.1.2 (`MAX_CONCURRENT_STREAMS` ↔ our `cap`), §6.9 (windows).
- QUIC RFC 9000 §3.5 (STOP_SENDING / RESET_STREAM crossing), §4 (flow control).
- RFC 9221 §5 (datagram flow control).

## Where we deliberately differ

- **No wire-level connection window.** yamux has none; HTTP/2, QUIC and rust-yamux do. The disconfirmer: a shared *wire* window lets one unread flow stall every flow (Nerdbank v2's reason for per-channel windows). We bound memory **locally** instead (smux-style `budget`). A peer that keeps sending into a paused side just waits on its per-flow credit. Revisit only with a new `wire_version`, plus cancellation reclamation and fair scheduling.
- **Explicit ACCEPT before DATA**: costs one RTT per flow, but buys refusal before bytes (SSH, Nerdbank).
- **Credit returns on `consume`, not receipt** (h2, SSH).
- **Odd/even ids by side, never reused**, with skipped lower ids retired (HTTP/2, yamux).
- **Peer violations are fatal to the tunnel** (yamux): an overrun or an empty TCP DATA frame means a broken or hostile peer.
- **TCP head-of-line blocking is accepted.** Per-flow credit stops one *slow reader* from blocking others, but a lost segment on the one TCP tunnel still delays every flow. Fixing that takes a different transport (QUIC), not another frame type.
- **Sans-IO** removes the writer/mutex deadlock class seen in yamux and smux. It doesn't give fairness, deadlines or buffer bounds for free, which is why the control queue, round-robin lanes, `tick`/`expire` and `budget` exist.

## Tests: the tripwires

Keep covered:
- aggregate credit across several stalled flows (`budget`);
- RESET or discard with unread bytes;
- STOP while a writer is blocked;
- a burst of tiny frames (the linear parser, no quadratic rescans);
- an unacknowledged OPEN expiring;
- CLOSE/WINDOW crossing;
- unknown RESET codes from a non-Rust peer;
- `grow` on a pending flow (a bug found and fixed in round 2; test `early`).
- a rejected header behind a spent budget is fatal, not a pause (`poison`, `guilty`; round 3);
- a STOP the peer never answers with CLOSE expires (`unanswered`; round 3).
