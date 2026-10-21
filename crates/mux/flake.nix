{
  description = "pdbd mux — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    output = { # expected build output per system; `lefthook run build:update` rewrites it
      "x86_64-linux" = "sha256-3zi4Wm9jUXNYQY0vtXBO1bB/xYRrzrN5bw8yxq30abk=";
    };
    packages = builtins.mapAttrs (system: packages: {
      default = packages.mux;
    }) root.packages;
  };
}
