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

        services =
          let
            tools = pkgs.buildEnv {
              name = "pdbd.tools";
              paths = [ pkgs.coreutils pkgs.bash pkgs.iproute2 pkgs.socat pkgs.ppp ]; # units need ip/socat/pppd/sh
            };
            names = lib.filter
              (n: builtins.match ".*\\.(service|target)" n != null)
              (builtins.attrNames (builtins.readDir ./ci/systemd));
            render = name: pkgs.writeText "unit"
              (builtins.replaceStrings [ "{{bin}}" ] [ "${tools}/bin" ] # absolute — systemd ExecStart requires it
                (builtins.readFile (./ci/systemd + "/${name}")));
          in
          pkgs.runCommand "pdbd.services" { } (''
            mkdir -p "$out"
          '' + lib.concatMapStrings (n: ''cp ${render n} "$out/${n}"'' + "\n") names);

        ci = pkgs.writeShellScriptBin "ci" ''
          set -eu
          hook="''${1:-ci}"
          root=$(${pkgs.git}/bin/git rev-parse --show-toplevel)
          export LEFTHOOK_CONFIG="$root/ci/lefthook.yml" # config lives under ci/, not the repo root
          export IP=${pkgs.iproute2}/bin/ip
          export PING=${pkgs.iputils}/bin/ping
          export SYSTEMCTL=${pkgs.systemd}/bin/systemctl
          needs_units=
          case "$hook" in ci | test | test:transport) needs_units=1 ;; esac
          case ",''${LEFTHOOK_EXCLUDE:-}," in *,privileged,*) needs_units= ;; esac
          have=$(${pkgs.coreutils}/bin/ls -A ${services} 2>/dev/null || true)
          reap() {
            { [ -n "$needs_units" ] && [ -n "$have" ]; } || return 0
            for u in ${services}/*; do sudo -n ${pkgs.coreutils}/bin/rm -f "/run/systemd/system/$(${pkgs.coreutils}/bin/basename "$u")" || true; done
            sudo -n "$SYSTEMCTL" daemon-reload || true
          }
          trap reap EXIT
          if [ -n "$needs_units" ] && [ -n "$have" ]; then
            sudo -n ${pkgs.coreutils}/bin/install -m0644 -t /run/systemd/system ${services}/*
            sudo -n "$SYSTEMCTL" daemon-reload
          fi
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
          ] ++ lib.optionals pkgs.stdenv.isLinux [ ci pkgs.iproute2 pkgs.iputils ]; # datapath harness is systemd/Linux-only
        };
      });
}
