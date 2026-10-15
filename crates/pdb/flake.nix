{
  description = "pdbd pdb — cargo build repro";

  inputs.root.url = "path:../..";

  outputs = { root, ... }: {
    packages = builtins.mapAttrs (system: packages: {
      default = packages.pdb;
      inherit (packages) toolchain;
    }) root.packages;
  };
}
