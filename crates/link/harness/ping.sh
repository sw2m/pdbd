#!/usr/bin/env bash
# Privileged ping test for the pdbd link (#12-A): bring up `run_tun` (the `linkup`
# example, in a network namespace) against the reference `pppd` over a socat PTY
# pair, N times, and ping across the link. Proves the clean-link datapath end to
# end: serial ↔ ppproto ↔ kernel TUN ↔ ping, interoperating with the reference impl.
#
# Requires: cargo + a Rust toolchain, socat, pppd, and sudo (TUN + pppd need root).
# Linux only. Not in plain CI (privileged); run via `lefthook run link`, or directly.
set -uo pipefail
N="${N:-10}"
ROOT=$(git rev-parse --show-toplevel)
cargo build -q -p link --example linkup
LINKUP=$(find "$ROOT/target" -type f -name linkup -path '*debug/examples*' | head -1)
[ -x "$LINKUP" ] || { echo "linkup example not built"; exit 1; }

run_once() {
  sudo -n pkill -x pppd 2>/dev/null; sudo -n pkill -x linkup 2>/dev/null
  sudo -n pkill -x socat 2>/dev/null; sudo -n ip netns del nsL 2>/dev/null; sleep 0.5
  rm -f /tmp/ppp_e2e_a /tmp/ppp_e2e_b
  socat PTY,link=/tmp/ppp_e2e_a,raw,echo=0 PTY,link=/tmp/ppp_e2e_b,raw,echo=0 & local s=$!
  sleep 0.5
  local a b; a=$(readlink -f /tmp/ppp_e2e_a); b=$(readlink -f /tmp/ppp_e2e_b)
  # pppd first (it retransmits, so it is listening); then pdbd's linkup.
  sudo -n pppd "$b" 115200 noauth local nodetach nocrtscts novj noccp \
    lcp-echo-interval 0 10.0.0.2:10.0.0.1 >/dev/null 2>&1 &
  sleep 2
  sudo -n ip netns add nsL 2>/dev/null
  sudo -n ip netns exec nsL "$LINKUP" "$a" 10.0.0.1 10.0.0.2 pdbd0 >/dev/null 2>&1 &
  local up=no i
  for i in $(seq 1 15); do
    ip -4 addr show ppp0 2>/dev/null | grep -q 10.0.0.2 && { up=yes; break; }
    sleep 1
  done
  local res=FAIL
  [ "$up" = yes ] && ping -c3 -W2 10.0.0.1 >/dev/null 2>&1 && res=PASS
  kill "$s" 2>/dev/null
  sudo -n pkill -x linkup 2>/dev/null; sudo -n pkill -x pppd 2>/dev/null
  sudo -n pkill -x socat 2>/dev/null; sudo -n ip netns del nsL 2>/dev/null
  echo "$res"
}

pass=0
for n in $(seq 1 "$N"); do
  r=$(run_once)
  echo "run $n: $r"
  [ "$r" = PASS ] && pass=$((pass + 1))
done
echo "PING E2E: $pass/$N PASS"
[ "$pass" = "$N" ]
