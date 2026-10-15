{ pkgs, rust, root, crate ? null }:
let
  inherit (pkgs) lib;
  platform = pkgs.makeRustPlatform { cargo = rust; rustc = rust; };
  workspace = lib.importTOML (root + "/Cargo.toml");
  sources = member: map (part: lib.fileset.maybeMissing (root + "/${member}/${part}")) [
    "Cargo.toml"
    "build.rs"
    "src"
    "examples"
    "tests"
    "benches"
  ];
  members = if crate == null then workspace.workspace.members else [ "crates/${crate}" ];
  has = path: lib.any (member: builtins.pathExists (root + "/${member}/${path}")) members;
  manifests = map (member: lib.importTOML (root + "/${member}/Cargo.toml")) members;
  targets =
    lib.optional (lib.any (m: m ? lib) manifests || has "src/lib.rs") "--lib"
    ++ lib.optional (lib.any (m: m ? bin) manifests || has "src/main.rs") "--bins"
    ++ lib.optional (has "examples") "--examples";
  rlib = if crate == null then "lib*.rlib" else "lib${lib.replaceStrings [ "-" ] [ "_" ] crate}.rlib";
in
platform.buildRustPackage {
  pname = if crate == null then "pdbd-workspace" else crate;
  version = workspace.workspace.package.version;
  src = lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.unions [
      (root + "/Cargo.toml")
      (root + "/Cargo.lock")
      (lib.fileset.unions (lib.concatMap sources workspace.workspace.members))
      (root + "/proto")
    ];
  };
  cargoLock = {
    lockFile = root + "/Cargo.lock";
    outputHashes = {
      "ppproto-0.2.1" = "sha256-JhgNEz02hFF/Jd0Ikf+IsH3wA/WBcsQabNYJVQA9V8k="; # #45: sw2m fork rev pinned in Cargo.toml [patch]
    };
  };
  cargoBuildFlags = (if crate == null then [ "--workspace" ] else [ "-p" crate ]) ++ targets;
  doCheck = false;
  installPhase = ''
    runHook preInstall
    release=target/${pkgs.stdenv.hostPlatform.rust.cargoShortTarget}/release
    mkdir -p $out/lib $out/bin $out/examples
    find "$release" -maxdepth 1 -type f -name '${rlib}' -exec cp {} $out/lib/ \;
    find "$release" -maxdepth 1 -type f -executable -exec cp {} $out/bin/ \;
    [ ! -d "$release/examples" ] || find "$release/examples" -maxdepth 1 -type f -executable -not -name '*-*' -exec cp {} $out/examples/ \;
    [ -n "$(find $out -type f)" ] || { echo "no build outputs for ${if crate == null then "the workspace" else crate}"; exit 1; }
    runHook postInstall
  '';
}
