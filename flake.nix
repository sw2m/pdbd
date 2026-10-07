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

        # the process-compose client (socket pre-wired); see README "Local
        # development" for why the scheduler runs as a lefthook sibling.
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

        # the CI entrypoint — see README "Local development".
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
            pkgs.process-compose
            services
            ci
            # transport test deps (services.yml) — #12
            pkgs.socat
            pkgs.ppp
            pkgs.iproute2
          ];
        };
      });
}
