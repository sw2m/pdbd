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
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        rust = pkgs.rust-bin.stable."1.88.0".default; # kept in step with the crate toolchain
        # pinned to the rig's portable tooling/bin/lefthook (byte-identical release binary) so CI and the git-hook shims run one lefthook
        lefthook = let
          v = "2.1.15";
          plat = {
            "x86_64-linux" = { arch = "x86_64"; sha256 = "df981b477c236546435047fb8c8ee1f6f902e17faebb1b07b6bf2165832e8fe3"; };
            "aarch64-linux" = { arch = "arm64"; sha256 = "f3c881e1b6aa7f0461970173b1d692e0eb026bddfcd051cc87f636bf9a9782e0"; };
          }.${system};
        in pkgs.stdenvNoCC.mkDerivation {
          pname = "lefthook";
          version = v;
          src = pkgs.fetchurl {
            url = "https://github.com/evilmartians/lefthook/releases/download/v${v}/lefthook_${v}_Linux_${plat.arch}";
            inherit (plat) sha256;
          };
          dontUnpack = true;
          installPhase = "install -Dm755 $src $out/bin/lefthook";
          meta.mainProgram = "lefthook";
        };
        # prettier + the maintained prettier-plugin-toml, pinned via nix/prettier's
        # package-lock.json so `prettier` resolves the TOML plugin on PATH hermetically
        # (no network / global npm at CI time). The wrapper passes the plugin by its
        # absolute store path, so no .prettierrc plugin-resolution step is needed.
        prettier = pkgs.buildNpmPackage {
          pname = "pdbd-prettier";
          version = "0.0.0";
          src = ./nix/prettier;
          npmDepsHash = "sha256-Z/rSe99x04f/18mQpYYceBckNm1oYNBrIRWeQm9JauI="; # prefetch-npm-deps nix/prettier/package-lock.json
          dontNpmBuild = true;
          nativeBuildInputs = [ pkgs.makeWrapper ];
          installPhase = ''
            runHook preInstall
            mkdir -p $out/lib/pdbd-prettier $out/bin
            cp -r node_modules $out/lib/pdbd-prettier/node_modules
            makeWrapper ${pkgs.nodejs}/bin/node $out/bin/prettier \
              --add-flags "$out/lib/pdbd-prettier/node_modules/prettier/bin/prettier.cjs" \
              --add-flags "--plugin=$out/lib/pdbd-prettier/node_modules/prettier-plugin-toml/lib/index.cjs"
            runHook postInstall
          '';
          meta.mainProgram = "prettier";
        };
      in {
        devShells.default = pkgs.mkShell {
          packages = [
            rust
            pkgs.buf
            lefthook
            prettier # prettier + prettier-plugin-toml for the format:root CI check
            pkgs.git
            pkgs.pkg-config
            pkgs.yq-go # structural YAML reader for the lefthook fan-out (lefthook.yml)
          ];
        };
      });
}
