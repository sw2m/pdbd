#!/bin/sh
# #62
set -eu
top=$(git rev-parse --show-toplevel)
dir=$1
flake="git+file://$top?shallow=1"
[ "$dir" = . ] || flake="$flake&dir=$dir"
sums="$top/$dir/output.sha256"
system=$(nix eval --impure --raw --expr builtins.currentSystem)
build() { nix build --no-link --print-out-paths --option sandbox-fallback false "$@" "$flake"; }

case "${2:-check}" in
  update)
    if [ "$dir" = . ]; then nix flake lock "$flake"; else nix flake update --flake "$flake"; fi
    out=$(build)
    sum=$(nix hash path "$out")
    { grep -v "^$system " "$sums" 2>/dev/null || true; echo "$system $sum"; } | sort > "$sums.new"
    mv "$sums.new" "$sums"
    ;;
  check)
    pins() { yq -p json -o tsv '.nodes[].locked.narHash // ""' "$1" | sort -u; }
    [ "$dir" = . ] || [ "$(pins "$top/$dir/flake.lock")" = "$(pins "$top/flake.lock")" ] \
      || { echo "$dir: flake.lock pins differ from the root's; run build:update" >&2; exit 1; }
    out=$(build --no-update-lock-file)
    build --no-update-lock-file --rebuild >/dev/null
    tool=$(nix eval --no-update-lock-file --raw "$flake#toolchain.outPath")
    want=$(nix eval --no-update-lock-file --raw "git+file://$top?shallow=1#toolchain.outPath")
    [ "$tool" = "$want" ] || { echo "$dir: toolchain $tool != root's $want; run build:update" >&2; exit 1; }
    sum=$(nix hash path "$out")
    recorded=$(awk -v s="$system" '$1 == s { print $2 }' "$sums" 2>/dev/null || true)
    [ "$sum" = "$recorded" ] || { echo "$dir: output $sum != recorded ${recorded:-<none>} for $system" >&2; exit 1; }
    ;;
esac
