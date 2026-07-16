# Action Items

### Critical

_None._

### Important

_None._

### Suggestions

- [ ] [crates/dh-vmm/src/lib.rs:1] Pre-existing (unchanged by this diff): the `#![deny(unsafe_code)]` comment says allows are "in kvm.rs only", but allows live in 7 modules (kvm, inject, tsc, boundary, runctl, msr, run). Not a regression — only reword if a future change touches this header; consider "targeted allows in the x86_64 KVM modules".
- [ ] [local-tooling] Optional: configure a `.cargo/config.toml` aarch64 linker or install `qemu-user-static`+`binfmt` so `cargo test --target aarch64` can actually *run* the determinism math locally (compilation already succeeds; CI arm runner already runs it natively). Purely for local-confidence convenience — no defect.
