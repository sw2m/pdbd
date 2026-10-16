#!/bin/sh
# see crates/mux/lefthook.yml (benchmark) and crates/mux/CLAUDE.md §"Benchmarks"
set -eu
here=$(cd "$(dirname "$0")" && pwd)
peers=$(mktemp -d)
(cd "$here/go" && GOTOOLCHAIN=local go run . >"$peers/go.tsv")
(cd "$here/dotnet" && dotnet run -c Release >"$peers/dotnet.tsv")
MUX_PEERS=$peers cargo bench -p mux --bench compare --locked
