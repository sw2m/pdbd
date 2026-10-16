#!/bin/sh
# see crates/mux/lefthook.yml (benchmark) and crates/mux/CLAUDE.md §"Benchmarks"
set -eu
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
out=$(mktemp -d)
cp -R "$here/go" "$work/go"
cd "$work/go"
export GOTOOLCHAIN=auto
go get $(grep -v '^#' versions)
go mod tidy
go run . >"$out/go.tsv"
cd "$here/dotnet"
dotnet restore --force-evaluate
dotnet run -c Release --no-restore >"$out/dotnet.tsv"
cd "$here/rust"
cargo update
MUX_PEERS=$out cargo bench --bench compare
tools="$(rustc --version) · $(go version) · .NET SDK $(dotnet --version)"
echo "toolchains: $tools"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  printf '\nToolchains: %s\n' "$tools" >>"$GITHUB_STEP_SUMMARY"
fi
