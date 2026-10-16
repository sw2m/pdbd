#!/bin/sh
# see crates/mux/lefthook.yml (benchmark) and crates/mux/CLAUDE.md §"Benchmarks"
set -eu
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
out=$(mktemp -d)
pins() {
  if [ -f "$1" ]; then grep -v '^#' "$1" | grep . || true; fi
}

cp -R "$here/go" "$work/go"
cd "$work/go"
export GOTOOLCHAIN=auto
pins "$here/go/pins" | while read -r module version; do go get "$module@$version"; done
go mod tidy
go run . >"$out/go.tsv"

cd "$here/dotnet"
pin=$(pins pins | awk 'NR == 1 { print "[" $2 "]" }')
dotnet restore --force-evaluate -p:Pin="${pin:-*}"
dotnet run -c Release --no-restore -p:Pin="${pin:-*}" >"$out/dotnet.tsv"

cd "$here/rust"
cargo update
pins pins | while read -r name version; do cargo update -p "$name" --precise "$version"; done
MUX_PEERS=$out MUX_TABLE=$out/table.md cargo bench --bench compare

{
  printf '### mux\n\n'
  cat "$out/table.md"
  printf '\nToolchains: %s · %s · .NET SDK %s.\n' "$(rustc --version)" "$(go version)" "$(dotnet --version)"
} >>"${BENCHMARK_REPORT:-/dev/stdout}"
