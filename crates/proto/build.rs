use std::path::PathBuf;

// Pure-Rust codegen: protox compiles the .proto set to a FileDescriptorSet (no
// external protoc), which tonic-prost-build turns into the prost + tonic types.
// buf remains the proto governance gate (lint/breaking) — see harness/ci/check-proto.sh.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?)
        .join("..")
        .join("..")
        .join("proto");

    let files = [
        "pdbd/v1/common.proto",
        "pdbd/v1/tunnel.proto",
        "pdbd/v1/command.proto",
        "pdbd/v1/socat.proto",
        "pdbd/v1/service.proto",
    ];
    let paths: Vec<PathBuf> = files.iter().map(|f| root.join(f)).collect();

    let fds = protox::compile(&paths, [&root])?;
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_fds(fds)?;

    println!("cargo:rerun-if-changed={}", root.display());
    Ok(())
}
