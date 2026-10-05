// Scaffold entrypoint — no link yet. References a generated type so the build
// proves the pdbd.v1 codegen linked end to end.
fn main() {
    let _ = pdbd_proto::v1::HelloRequest::default();
    println!("pdbd {}", env!("CARGO_PKG_VERSION"));
}
