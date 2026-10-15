{
  description = "pdbd transport — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    output = { # expected build output per system; `lefthook run build:update` rewrites it
      "x86_64-linux" = "sha256-08qjM94O1Z/n9orhbXibS4EBHMLQa5d5VNbASganevA=";
    };
    packages = builtins.mapAttrs (system: packages: {
      default = packages.transport;
    }) root.packages;
  };
}
