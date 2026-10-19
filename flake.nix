{
  description = "pdbd — transport-agnostic debug/RPC bridge";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    bencher.url = "github:sw2m/bencher/v0.2.0"; # see README "Benchmarks"
    pdbd-bench = {
      url = "github:sw2m/pdbd-bench/feat/socat"; # sw2m/pdbd-bench#3, until it merges
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, bencher, pdbd-bench }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        rust = pkgs.rust-bin.stable."1.88.0".default; # the repo's one toolchain; crate flakes take it from here (#62)
        build = crate: import ./nix/crate.nix { inherit pkgs rust crate; root = ./.; };
        crates = map baseNameOf (pkgs.lib.importTOML ./Cargo.toml).workspace.members;
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
        prettier = let
          npm = name: version: sha512: pkgs.fetchurl {
            url = "https://registry.npmjs.org/${name}/-/${baseNameOf name}-${version}.tgz";
            hash = "sha512-${sha512}";
          };
          pluginModules = pkgs.runCommand "prettier-plugin-toml-modules" { } ''
            mkdir -p $out/node_modules/@tombi-toml $out/node_modules/@dual-bundle
            unpack() {
              local tmp; tmp=$(mktemp -d)
              tar -xzf "$1" -C "$tmp"
              mv "$tmp/package" "$2"
            }
            unpack ${npm "prettier-plugin-toml" "3.0.2" "1t1FOSGaXNy7xN6ffE7ND71VqsnbUtanGEKAD1hTwFDrGIH1cwZ0KH6lMThDVItjdV/UUJWqD3f1+16AHG3eUQ=="} $out/node_modules/prettier-plugin-toml
            unpack ${npm "smol-toml" "1.9.1" "j7WyPdf0CJoutMzMoPLF/bVXHtKZIT9Rb4eGeEbWw8X2t5oirjPgpL+OpaENZrILYaXqYwnVH+qgUwVbl+pUJQ=="} $out/node_modules/smol-toml
            unpack ${npm "@tombi-toml/wasm-lib" "1.8.3" "aUUT/HK8sT0eGJn60S55dvG0ts/lXQr+AshrwFb+45LVUjps9xx6xLJrkC2fupZTLK0tHGTIV3lG9C3YG86PyA=="} $out/node_modules/@tombi-toml/wasm-lib
            unpack ${npm "@dual-bundle/import-meta-resolve" "4.2.1" "id+7YRUgoUX6CgV0DtuhirQWodeeA7Lf4i2x71JS/vtA5pRb/hIGWlw+G6MeXvsM+MXrz0VAydTGElX1rAfgPg=="} $out/node_modules/@dual-bundle/import-meta-resolve
          '';
        in pkgs.runCommand "pdbd-prettier" { nativeBuildInputs = [ pkgs.makeWrapper ]; } ''
          makeWrapper ${pkgs.prettier}/bin/prettier $out/bin/prettier \
            --add-flags "--plugin=${pluginModules}/node_modules/prettier-plugin-toml/lib/index.cjs"
        '';
      in {
        packages = pkgs.lib.genAttrs crates build // {
          default = build null;
          toolchain = rust;
        };
        devShells.benchmark = let
          musl = "${pkgs.stdenv.hostPlatform.parsed.cpu.name}-unknown-linux-musl";
        in pkgs.mkShell {
          packages = [
            (rust.override { targets = [ musl ]; })
            bencher.packages.${system}.default
            pkgs.jq
          ];
          MUSL_TARGET = musl;
          BENCH_FIXTURE = pdbd-bench;
          BENCH_SETUP = "${bencher}/ci/setup.sh";
        };
        devShells.default = pkgs.mkShell {
          packages = [
            rust
            pkgs.buf
            lefthook
            prettier
            pkgs.git
            pkgs.pkg-config
            pkgs.yq-go
            pkgs.minijinja
            pkgs.iproute2
            pkgs.iputils
            pkgs.socat
            pkgs.ppp
            pkgs.bash
            pkgs.util-linux # nsenter: ip netns exec remounts /sys, unsafe in containers
          ];
        };
      }) // {
        output = { # expected build output per system; `lefthook run build:update` rewrites it
          "x86_64-linux" = "sha256-uRi8s4VJjnq2pk9Ap/nwAyyRJ9GHGBdVwpDXQJ+6+uQ=";
        };
      };
}
