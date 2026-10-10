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
        inherit (pkgs) lib;
        rust = pkgs.rust-bin.stable."1.88.0".default; # kept in step with the crate toolchain
        onLinux = pkgs.stdenv.isLinux;

        units =
          let
            tools = pkgs.buildEnv {
              name = "pdbd-ci-tools";
              paths = [ pkgs.coreutils ]; # tests extend this with their own binaries
            };
            names = lib.filter
              (n: builtins.match ".*\\.(service|target)" n != null)
              (builtins.attrNames (builtins.readDir ./ci/systemd));
            render = name: pkgs.writeText "unit"
              (builtins.replaceStrings [ "{{bin}}" ] [ "${tools}/bin" ] # absolute — systemd ExecStart requires it
                (builtins.readFile (./ci/systemd + "/${name}")));
            dir = pkgs.runCommand "pdbd-units" { } (''
              mkdir -p "$out"
            '' + lib.concatMapStrings (n: ''cp ${render n} "$out/${n}"'' + "\n") names);
          in
          {
            install = pkgs.writeShellScriptBin "pdbd-units-install" ''
              set -eu
              [ -n "$(${pkgs.coreutils}/bin/ls -A ${dir} 2>/dev/null)" ] || exit 0 # no-op when no units
              sudo -n ${pkgs.coreutils}/bin/install -m0644 -t /run/systemd/system ${dir}/*
              sudo -n ${pkgs.systemd}/bin/systemctl daemon-reload
            '';
            uninstall = pkgs.writeShellScriptBin "pdbd-units-uninstall" ''
              set -eu
              [ -n "$(${pkgs.coreutils}/bin/ls -A ${dir} 2>/dev/null)" ] || exit 0
              for u in ${dir}/*; do
                sudo -n ${pkgs.coreutils}/bin/rm -f "/run/systemd/system/$(${pkgs.coreutils}/bin/basename "$u")" || true
              done
              sudo -n ${pkgs.systemd}/bin/systemctl daemon-reload || true
            '';
          };

        ci = pkgs.writeShellScriptBin "ci" ''
          set -eu
          hook="''${1:-ci}"
          root=$(${pkgs.git}/bin/git rev-parse --show-toplevel)
          export LEFTHOOK_CONFIG="$root/ci/lefthook.yml" # config lives under ci/, not the repo root
          ${units.install}/bin/pdbd-units-install
          trap '${units.uninstall}/bin/pdbd-units-uninstall >/dev/null 2>&1 || true' EXIT
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
          ] ++ lib.optionals onLinux [ ci ];
        };
      });
}
