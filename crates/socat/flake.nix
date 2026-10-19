{
  description = "pdbd socat — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    output = { # expected build output per system; `lefthook run build:update` rewrites it
      "x86_64-linux" = "sha256-Cwv8d4e0PCOBH28IZbG90C70xsOnfmJg/8OcdHXIlS8=";
    };
    packages = builtins.mapAttrs (system: packages: {
      default = packages.socat;
    }) root.packages;
  };
}
