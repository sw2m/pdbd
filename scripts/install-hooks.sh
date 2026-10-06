#!/usr/bin/env sh
# Install a local, gitignored pre-commit hook that runs the lefthook CI checks inside
# the pinned nix environment. Pick a mode; see README "Local development".
#   ./scripts/install-hooks.sh straight   # nix is installed on the host
#   ./scripts/install-hooks.sh podman     # nix runs in a rootless podman container
set -eu

root=$(git rev-parse --show-toplevel)
hooks="$root/.githooks"
mkdir -p "$hooks"

case "${1:-}" in
  straight)
    cat > "$hooks/pre-commit" <<'HOOK'
#!/usr/bin/env sh
exec nix develop -c lefthook run ci
HOOK
    ;;
  podman)
    # Default rootless mapping (container-root ↔ your uid) lines the bind-mount's
    # ownership up — no --userns=keep-id. A worktree keeps its git metadata OUTSIDE
    # the working dir, so mount the common git dir too (both at their real paths so
    # the .git pointer resolves), and hand nix a `path:` ref so it does not re-fetch
    # the tree over git.
    cat > "$hooks/pre-commit" <<'HOOK'
#!/usr/bin/env sh
set -eu
top=$(git rev-parse --show-toplevel)
common=$(git rev-parse --git-common-dir)
case "$common" in /*) ;; *) common="$top/$common" ;; esac
exec podman run --rm \
  -v "$top":"$top" -v "$common":"$common" -w "$top" -v pdbd-nix:/nix \
  docker.io/nixos/nix \
  nix --extra-experimental-features 'nix-command flakes' develop "path:$top" -c lefthook run ci
HOOK
    ;;
  *)
    echo "usage: $0 <straight|podman>" >&2
    exit 2
    ;;
esac

chmod +x "$hooks/pre-commit"
git config core.hooksPath .githooks
echo "installed ${1} pre-commit hook → .githooks/pre-commit (core.hooksPath=.githooks)"
