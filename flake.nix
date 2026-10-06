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
      in {
        devShells.default = pkgs.mkShell {
          packages = [
            rust
            pkgs.buf
            pkgs.lefthook
            pkgs.git
            pkgs.pkg-config
            # privileged transport ping test (lefthook.transport.yml): a virtual
            # serial pair, the reference PPP peer, and TUN/route tooling. #12.
            pkgs.socat
            pkgs.ppp
            pkgs.iproute2
          ];
        };
      });
}
