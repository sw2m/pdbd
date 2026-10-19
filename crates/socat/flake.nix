{
  description = "pdbd socat — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    output = { # expected build output per system; `lefthook run build:update` rewrites it
      "x86_64-linux" = "sha256-Q5Hef5cYGPbVyO202MpzVoRlPoxRN9NPxzfcngz67n0=";
    };
    packages = builtins.mapAttrs (system: packages: {
      default = packages.socat;
    }) root.packages;
  };
}
