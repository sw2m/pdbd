{
  description = "pdbd mux — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    output = { # expected build output per system; `lefthook run build:update` rewrites it
      "x86_64-linux" = "sha256-UgfjunV4excBKEOv/ieCgykRVBlQ0pwUD4IRctTEadE=";
    };
    packages = builtins.mapAttrs (system: packages: {
      default = packages.mux;
    }) root.packages;
  };
}
