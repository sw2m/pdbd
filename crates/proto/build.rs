use std::path::PathBuf;

// protoc-free codegen (protox → tonic-prost-build), so only the Cargo toolchain is
// needed to build. buf stays the proto governance gate — see harness/ci/check-proto.sh.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?)
        .join("..")
        .join("..")
        .join("proto");

    // Gather the .proto set by walking the tree (a new proto can't silently drop out
    // of codegen) and rerun per file (a directory trigger misses nested edits).
    let mut protos = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "proto") {
                protos.push(path);
            }
        }
    }
    protos.sort();
    for proto in &protos {
        println!("cargo:rerun-if-changed={}", proto.display());
    }

    let fds = protox::compile(&protos, [&root])?;
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_fds(fds)?;
    Ok(())
}
