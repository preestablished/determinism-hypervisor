#![forbid(unsafe_code)]

//! Thin bin entry: the real CLI lives in the lib's x86_64-gated `cli`
//! module (everything it drives — KVM, VMX exits, the INST_RETIRED
//! counter — is x86_64-only; bead v5w keeps the crate buildable on
//! other arches so `--workspace` CI legs need no exclude list).

fn main() {
    #[cfg(target_arch = "x86_64")]
    dh_cli::cli::main();

    #[cfg(not(target_arch = "x86_64"))]
    {
        eprintln!("dh-cli requires an x86_64 host (KVM/VMX, ARCH §1)");
        std::process::exit(2);
    }
}
