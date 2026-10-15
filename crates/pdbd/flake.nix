{
  description = "pdbd pdbd — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    output = { # expected build output per system; `lefthook run build:update` rewrites it
      "x86_64-linux" = "sha256-VESd5Hv6Xjjtzm3CJr/ihUHUgcOETJgZ1WJ5I5YRe7M=";
    };
    packages = builtins.mapAttrs (system: packages: {
      default = packages.pdbd;
    }) root.packages;
  };
}
