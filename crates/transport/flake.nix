{
  description = "pdbd transport — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    packages = builtins.mapAttrs (system: packages: {
      default = packages.transport;
      inherit (packages) toolchain;
    }) root.packages;
  };
}
