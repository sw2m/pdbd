{
  description = "pdbd pdbd — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    packages = builtins.mapAttrs (system: packages: {
      default = packages.pdbd;
      inherit (packages) toolchain;
    }) root.packages;
  };
}
