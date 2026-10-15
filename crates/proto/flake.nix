{
  description = "pdbd proto — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    packages = builtins.mapAttrs (system: packages: {
      default = packages.proto;
      inherit (packages) toolchain;
    }) root.packages;
  };
}
