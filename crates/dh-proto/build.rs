fn main() -> Result<(), Box<dyn std::error::Error>> {
    // No protoc on dev boxes or CI runners; use the vendored binary (same
    // pattern as ../snapshot-store's snapstore-client — proto-seam decision).
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);
    tonic_build::configure()
        .build_server(true) // dh-worker implements HypervisorWorker
        .build_client(true) // control-plane/orchestrator consume it
        .compile_protos(&["../../proto/hypervisor.proto"], &["../../proto"])?;
    Ok(())
}
