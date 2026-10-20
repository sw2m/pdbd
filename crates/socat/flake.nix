{
  description = "pdbd socat — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    output = { # expected build output per system; `lefthook run build:update` rewrites it
      "x86_64-linux" = "sha256-leU+CI/Y8Ks7Q4pHcbIalYYjB8g2aZIOkesqrNAlOoc=";
    };
    packages = builtins.mapAttrs (system: packages: {
      default = packages.socat;
    }) root.packages;
  };
}
