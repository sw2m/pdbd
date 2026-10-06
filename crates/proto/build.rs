use std::path::{Path, PathBuf};

// Pure-Rust codegen: protox compiles the .proto set to a FileDescriptorSet (no
// external protoc), which tonic-prost-build turns into the prost + tonic types.
// buf remains the proto governance gate (lint/breaking) — see harness/ci/check-proto.sh.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?)
        .join("..")
        .join("..")
        .join("proto");

    // Discover every .proto under the module root rather than hand-listing them, so
    // adding one never silently drops out of codegen; emit a per-file rerun trigger
    // (a directory rerun-if-changed does not reliably catch nested edits).
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
