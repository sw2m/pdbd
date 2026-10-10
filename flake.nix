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

        # --- systemd unit engine (generic; Linux-only) -----------------------
        # A test drops *.service / *.target into units/ and the engine picks them
        # up — it names no unit and no binary. The one substitution a unit carries
        # is {{bin}}: the CI toolchain's absolute bin dir. systemd requires an
        # absolute ExecStart, so a unit writes {{bin}}/ip, {{bin}}/pppd, …, and a
        # test makes those resolve by adding its packages to `paths` below. %i (the
        # systemd instance) passes through untouched — the engine never expands it.
        ciTools = pkgs.buildEnv {
          name = "pdbd-ci-tools";
          paths = [ pkgs.coreutils ]; # tests extend this with their own binaries
        };
        unitNames = lib.filter
          (n: builtins.match ".*\\.(service|target)" n != null)
          (builtins.attrNames (builtins.readDir ./units));
        renderUnit = name: pkgs.writeText "unit"
          (builtins.replaceStrings [ "{{bin}}" ] [ "${ciTools}/bin" ]
            (builtins.readFile (./units + "/${name}")));
        unitDir = pkgs.runCommand "pdbd-units" { } (''
          mkdir -p "$out"
        '' + lib.concatMapStrings (n: ''cp ${renderUnit n} "$out/${n}"'' + "\n") unitNames);

        # Install discovered units to /run/systemd/system (tmpfs → disposable) and
        # reload; a no-op when nothing was discovered, so PR-1-alone (no units) is
        # green and `ci` runs anywhere lefthook does.
        installUnits = pkgs.writeShellScriptBin "pdbd-units-install" ''
          set -eu
          [ -n "$(${pkgs.coreutils}/bin/ls -A ${unitDir} 2>/dev/null)" ] || exit 0
          sudo -n ${pkgs.coreutils}/bin/install -m0644 -t /run/systemd/system ${unitDir}/*
          sudo -n ${pkgs.systemd}/bin/systemctl daemon-reload
        '';
        uninstallUnits = pkgs.writeShellScriptBin "pdbd-units-uninstall" ''
          set -eu
          [ -n "$(${pkgs.coreutils}/bin/ls -A ${unitDir} 2>/dev/null)" ] || exit 0
          for u in ${unitDir}/*; do
            sudo -n ${pkgs.coreutils}/bin/rm -f "/run/systemd/system/$(${pkgs.coreutils}/bin/basename "$u")" || true
          done
          sudo -n ${pkgs.systemd}/bin/systemctl daemon-reload || true
        '';

        # CI entrypoint — see README "Local development". Installs discovered units,
        # runs lefthook, removes them on exit.
        ci = pkgs.writeShellScriptBin "ci" ''
          set -eu
          hook="''${1:-ci}"
          ${installUnits}/bin/pdbd-units-install
          trap '${uninstallUnits}/bin/pdbd-units-uninstall >/dev/null 2>&1 || true' EXIT
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
