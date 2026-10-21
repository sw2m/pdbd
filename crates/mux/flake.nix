{
  description = "pdbd mux — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    output = { # expected build output per system; `lefthook run build:update` rewrites it
      "x86_64-linux" = "sha256-gF5z8yXYNS9wFlF6LT2k/eTkA0Byqk4Sswe5vTG3WgY=";
    };
    packages = builtins.mapAttrs (system: packages: {
      default = packages.mux;
    }) root.packages;
  };
}
