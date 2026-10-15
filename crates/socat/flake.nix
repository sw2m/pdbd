{
  description = "pdbd socat — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    output = { # expected build output per system; `lefthook run build:update` rewrites it
      "x86_64-linux" = "sha256-1/isT3c5cQhwOkfg1It5AvDRtb+g31xGaoGlGHh5ZvI=";
    };
    packages = builtins.mapAttrs (system: packages: {
      default = packages.socat;
    }) root.packages;
  };
}
