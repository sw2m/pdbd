{
  description = "pdbd mux — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    output = { # expected build output per system; `lefthook run build:update` rewrites it
      "x86_64-linux" = "sha256-ORHcIAsBwz37peFasyKM3kX4AEmb2i6BBkCHVUjfzi0=";
    };
    packages = builtins.mapAttrs (system: packages: {
      default = packages.mux;
    }) root.packages;
  };
}
