fn main() -> Result<(), Box<dyn std::error::Error>> {
    // No protoc on dev boxes or CI runners; use the vendored binary (same
    // pattern as ../snapshot-store's snapstore-client — proto-seam decision).
    // set_var is unsafe from edition 2024; sound here on 2021 (build scripts
    // are single-threaded).
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);
    tonic_build::configure()
        .build_server(true) // dh-worker implements HypervisorWorker
        .build_client(true) // control-plane/orchestrator consume it
        .compile_protos(&["../../proto/hypervisor.proto"], &["../../proto"])?;

    // A `package` vs include_proto! mismatch otherwise surfaces as an opaque
    // "No such file" at lib.rs's include site; fail loudly here instead.
    let expected =
        std::path::Path::new(&std::env::var("OUT_DIR")?).join("determinism.hypervisor.v1.rs");
    if !expected.exists() {
        return Err(format!(
            "codegen produced no {} — proto `package` drifted from lib.rs include_proto!",
            expected.display()
        )
        .into());
    }
    Ok(())
}
