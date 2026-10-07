{
  description = "pdbd — transport-agnostic debug/RPC bridge";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        rust = pkgs.rust-bin.stable."1.88.0".default; # kept in step with the crate toolchain

        # `services` — the process-compose client, socket pre-wired. CI-background
        # services that must outlive a single lefthook job live here; lefthook
        # can't keep a process across a job boundary (it reaps the job subtree),
        # so the scheduler is started as lefthook's *sibling* by `ci` below and
        # driven from the jobs as a client. `services up` boots it idle
        # (--keep-project, every service `disabled`); `services process start/stop`
        # drive services on demand; `services down` reaps the whole project.
        services = pkgs.writeShellScriptBin "services" ''
          set -eu
          sock="/tmp/pdbd-pc-$(id -u).sock"
          if [ "''${1:-}" = up ]; then
            shift
            root=$(${pkgs.git}/bin/git rev-parse --show-toplevel)
            exec ${pkgs.process-compose}/bin/process-compose up \
              -f "$root/services.yml" -D --keep-project -U -u "$sock" -t=false "$@"
          fi
          exec ${pkgs.process-compose}/bin/process-compose "$@" -U -u "$sock"
        '';

        # `ci` — the CI entrypoint. Boots the scheduler as a sibling of lefthook
        # (so services survive across jobs), runs the hook, and reaps the
        # scheduler on exit (trap, so it fires on failure too). CI runs
        # `nix develop -c ci`; local hooks run `ci pre-commit` / `ci pre-push`.
        ci = pkgs.writeShellScriptBin "ci" ''
          set -eu
          hook="''${1:-ci}"
          ${services}/bin/services up >/dev/null 2>&1 || true
          trap '${services}/bin/services down >/dev/null 2>&1 || true' EXIT
          ${pkgs.lefthook}/bin/lefthook run "$hook"
        '';
      in {
        devShells.default = pkgs.mkShell {
          packages = [
            rust
            pkgs.buf
            pkgs.lefthook
            pkgs.git
            pkgs.pkg-config
            # the CI background-service scheduler + its wrappers (see `services`)
            pkgs.process-compose
            services
            ci
            # privileged transport test deps (services.yml): a virtual serial pair,
            # the reference PPP peer, and TUN/route tooling. #12.
            pkgs.socat
            pkgs.ppp
            pkgs.iproute2
          ];
        };
      });
}
