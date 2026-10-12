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
      in {
        devShells.default = pkgs.mkShell {
          packages = [
            rust
            pkgs.buf
            lefthook
            pkgs.git
            pkgs.pkg-config
            pkgs.yq-go # structural YAML reader for the lefthook fan-out (lefthook.yml)
            pkgs.minijinja # minijinja-cli — renders the unit templates (crates/transport/lefthook.yml)
            pkgs.iproute2
            pkgs.iputils
            pkgs.socat
            pkgs.ppp
            pkgs.bash
            pkgs.util-linux # nsenter — enters a netns without ip's /sys remount (container-safe)
          ];
        };
      });
}
