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

        # The datapath harness is systemd-driven, so it is Linux-only; on darwin
        # the devShell still builds and edits the crates, the privileged datapath
        # test just does not run (see README "Local development").
        onLinux = pkgs.stdenv.isLinux;
        systemd = import ./ci/systemd.nix pkgs;

        # CI entrypoint — see README "Local development". Installs the disposable
        # unit templates, runs lefthook, removes them on exit.
        ci = pkgs.writeShellScriptBin "ci" ''
          set -eu
          hook="''${1:-ci}"
          ${systemd.install}/bin/pdbd-units-install
          trap '${systemd.uninstall}/bin/pdbd-units-uninstall >/dev/null 2>&1 || true' EXIT
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
          ] ++ pkgs.lib.optionals onLinux [
            ci
            systemd.install
            systemd.uninstall
            # datapath test deps (ci/systemd.nix units + lefthook checks) — #12
            pkgs.socat
            pkgs.ppp
            pkgs.iproute2
          ];
        };
      });
}
