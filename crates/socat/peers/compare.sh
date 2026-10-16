#!/bin/sh
# see crates/socat/lefthook.yml (benchmark) and crates/socat/CLAUDE.md §"Benchmark"
set -eu
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
bin=$work/bin
mkdir -p "$bin" "$work/sandbox"
pin() { grep -v '^#' "$1" | head -n 1; }
export SSL_CERT_FILE="${SSL_CERT_FILE:-$(nix build --no-link --print-out-paths "${PKGS:?}#cacert")/etc/ssl/certs/ca-bundle.crt}"

ln -s "$(nix build --no-link --print-out-paths "$(pin "$here/socat/nixpkgs")#socat")/bin/socat" "$bin/socat"
socat=$("$bin/socat" -V | sed -n 's/^socat version \([^ ]*\).*/\1/p')
upstream=$(curl -fsSL http://www.dest-unreach.org/socat/download/ 2>/dev/null \
  | grep -o 'socat-[0-9][0-9.]*[0-9]\.tar\.gz' | sed 's/^socat-//; s/\.tar\.gz$//' | sort -uV | tail -n 1 || true)

websocat=$(pin "$here/websocat/version")
if [ "$websocat" = latest ]; then
  websocat=$(curl -fsSLI -o /dev/null -w '%{url_effective}' https://github.com/vi/websocat/releases/latest | sed 's|.*/||')
fi
curl -fsSL -o "$bin/websocat" "https://github.com/vi/websocat/releases/download/$websocat/websocat.$(uname -m)-unknown-linux-musl"
chmod +x "$bin/websocat"
curl -fsSL -o "$work/websocat.md" "https://raw.githubusercontent.com/vi/websocat/$websocat/doc.md"

export GOTOOLCHAIN=auto
mod=$(pin "$here/go/versions")
meta=$(cd "$here/go" && go mod download -json "$mod")
oittaa=$(printf '%s\n' "$meta" | sed -n 's/^[[:space:]]*"Version": "\(.*\)",*$/\1/p')
src=$(printf '%s\n' "$meta" | sed -n 's/^[[:space:]]*"Dir": "\(.*\)",*$/\1/p')
GOBIN="$work/oittaa" go install "${mod%@*}/cmd/socat@$oittaa"
ln -s "$work/oittaa/socat" "$bin/oittaa"
(cd "$here/go" && go build -o "$bin/corpus" .)

cd "$here/rust"
cargo update -q
cargo build -q --release
tocat=$(cargo pkgid -q tocat-core | sed 's/.*@//')
parse=$(cargo metadata -q --format-version 1 \
  | grep -o '"manifest_path":"[^"]*/tocat-core-[^"/]*/Cargo.toml"' | head -n 1 \
  | sed 's/^"manifest_path":"//; s|Cargo.toml"$|src/endpoint/parse.rs|')

if unshare -rn true 2>/dev/null; then
  set -- unshare -rn
elif sudo -n unshare -n true 2>/dev/null; then
  set -- sudo -n unshare -n setpriv --reuid="$(id -u)" --regid="$(id -g)" --clear-groups
else
  echo "socat benchmark: cannot make an empty network namespace (unshare -rn, sudo unshare -n)" >&2
  exit 1
fi
cd "$work/sandbox"
"$@" env \
  PEERS="$here" \
  SANDBOX="$work/sandbox" \
  ORACLE_SOCAT="$bin/socat" \
  ORACLE_WEBSOCAT="$bin/websocat" \
  ORACLE_OITTAA="$bin/oittaa" \
  OITTAA_CORPUS="$bin/corpus" \
  OITTAA_SRC="$src" \
  WEBSOCAT_DOC="$work/websocat.md" \
  TOCAT_PARSE="$parse" \
  VERSION_SOCAT="$socat" \
  VERSION_WEBSOCAT="$websocat" \
  VERSION_OITTAA="$oittaa" \
  VERSION_TOCAT="$tocat" \
  UPSTREAM_SOCAT="$upstream" \
  ${BENCHMARK_REPORT:+BENCHMARK_REPORT="$BENCHMARK_REPORT"} \
  ${BENCHMARK_BLESS:+BENCHMARK_BLESS="$BENCHMARK_BLESS"} \
  "$here/rust/target/release/peers"
