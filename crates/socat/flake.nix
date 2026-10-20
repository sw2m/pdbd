{
  description = "pdbd socat — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    output = { # expected build output per system; `lefthook run build:update` rewrites it
      "x86_64-linux" = "sha256-aW56Gqv2FgvC8Od6ogVIBMn5Ifsqtw2RFwTBrv3HfQY=";
    };
    packages = builtins.mapAttrs (system: packages: {
      default = packages.socat;
    }) root.packages;
  };
}
