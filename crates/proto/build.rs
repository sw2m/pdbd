use std::path::{Path, PathBuf};

// protoc-free codegen (protox → tonic-prost-build), so only the Cargo toolchain is
// needed to build. buf stays the proto governance gate — see harness/ci/check-proto.sh.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?)
        .join("..")
        .join("..")
        .join("proto");

    // Walk for the .proto set instead of hand-listing it (a new proto can't silently
    // drop out), with a per-file rerun (a directory trigger misses nested edits).
    let mut protos = Vec::new();
    collect_protos(&root, &mut protos)?;
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

fn collect_protos(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_protos(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "proto") {
            out.push(path);
        }
    }
    Ok(())
}
