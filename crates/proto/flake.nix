{
  description = "pdbd proto — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    output = { # expected build output per system; `lefthook run build:update` rewrites it
      "x86_64-linux" = "sha256-XOlCDs6pdCrjRi5yHwpW+BXFUhNy1uCtzVAN172zY+I=";
    };
    packages = builtins.mapAttrs (system: packages: {
      default = packages.proto;
    }) root.packages;
  };
}
