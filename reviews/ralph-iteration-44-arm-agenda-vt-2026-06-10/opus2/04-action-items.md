# Action Items — iteration 44 (Claude Opus, 2nd reviewer)

### Critical

_None._

### Important

_None._

### Suggestions

- [ ] [crates/dh-vmm/src/lib.rs:11-30] Collapse the 10 individually-gated `pub mod` lines into a
  single `#[cfg(target_arch = "x86_64")] mod x86 { ... } pub use x86::*;` (or `cfg_if!`) block so a
  future KVM module added without the cfg attribute can't silently break the arm lane. Apply the
  same to tools/dh-cli/src/lib.rs:7-22 (5 gated modules). Maintainability only; current code is correct.

- [ ] [tests/determinism/Cargo.toml:14-19] Move `kvm-ioctls`, `nanokernel`, `vm-memory`, `libc`
  under `[target.'cfg(target_arch = "x86_64")'.dev-dependencies]` for consistency with dh-vmm/dh-cli
  and to avoid compiling them on the arm lane where every using test target gates to empty. They are
  arm-buildable today (cross-check passes), so this is optional cleanup, not a fix.

- [ ] [tools/dh-cli/src/main.rs (non-x86 stub)] Add a one-line comment documenting that the
  all-or-nothing "requires an x86_64 host" stub intentionally blocks even the host-only
  `caps`/`cpuid-diff` summaries on arm (m0_missing_caps_summary() is itself arm-buildable). Optional;
  or route `caps` through on arm if a Spark-side use case appears.

- [ ] [.github/workflows/ci.yaml:38-39] No change needed now — note for future CI-timing work that the
  arm leg's build footprint grew this iteration (now builds dh-vmm/dh-cli/dh-worker/determinism-tests
  + runs nanokernel's nasm+rust-lld build.rs) since the `--exclude` list was dropped.
