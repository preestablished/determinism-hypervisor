# Action Items

### Critical

- [ ] **[.github/workflows/ci.yaml:17 + crates/dh-vmm/src/msr.rs:18-20]** The new `ubuntu-24.04-arm` host leg will fail to compile: `dh-vmm` imports x86_64-only `kvm-bindings` symbols (`kvm_msr_filter`, `kvm_msr_filter_range`, `KVM_MSR_FILTER_DEFAULT_DENY`, `KVM_MSR_FILTER_READ`, `KVM_MSR_FILTER_WRITE`) **unconditionally with no `cfg(target_arch)` gate**, and those symbols do not exist in `kvm-bindings` `arm64` bindings (verified: 0 occurrences in `arm64/bindings.rs`). `dh-vmm` is a workspace member so `cargo build --workspace` always builds it on arm. Choose one:
  - **(A)** Gate `dh-vmm`'s KVM modules behind `#[cfg(target_arch = "x86_64")]` (in `lib.rs`, gate `mod msr; mod kvm; mod run; mod vt;`) and ensure the portable math still builds on arm — larger change, track as its own bead.
  - **(B, fastest)** Drop the arm entry for now: revert the matrix to `runner: [ubuntu-latest]` and file a follow-up bead for arm portability.
  - **(C)** Scope the arm matrix entry to only the arch-portable crates via explicit `-p` flags (brittle; not recommended).
  - Whichever path: actually run a build targeting aarch64 (or push and watch the arm leg) before claiming green — the original "all green" was x86_64-only.

### Important

- [ ] **[.github/workflows/ci.yaml:1-5]** Add a least-privilege token to the self-hosted-runner workflow:
  ```yaml
  permissions:
    contents: read
  ```
  The `if:` fork guard is the primary control and is correct, but a write-capable `GITHUB_TOKEN` on a self-hosted public-repo job is unnecessary blast radius. Both jobs only checkout + build.

### Suggestions

- [ ] **[.github/workflows/ci.yaml:2-4]** Decide whether the duplicate `push` + `pull_request` triggers (double matrix runs per PR commit, plus a run on ralph's branch push *and* the merge-to-`main` push) are worth the self-hosted runner time. If not, scope `push:` to `branches: [main]` — but confirm this still gives ralph's loop the safety net it expects on iteration branches.
- [ ] **[.github/workflows/ci.yaml:1]** Add a `concurrency` group (`group: ci-${{ github.ref }}`, `cancel-in-progress: true`) to avoid stacking superseded runs on the Intel box.
- [ ] **[repo root]** Add a `rust-toolchain.toml` pinning the channel (+ `rustfmt`, `clippy` components) so `clippy -D warnings` doesn't go red on a future stable's new lint — fitting for a determinism-themed project. The `manual_is_multiple_of` lint fixed in this PR is itself an example of this drift.
- [ ] **[.github/workflows/ci.yaml:64]** (optional hardening) Strengthen the probe to `test -c /dev/kvm && test -r /dev/kvm` so a stale non-device file at that path can't pass the check. Current `-r`-only is acceptable.
- [ ] **[.github/workflows/ci.yaml:17]** Confirm the org has `ubuntu-24.04-arm` runner availability so a missing label doesn't leave the job queued indefinitely (moot if the arm leg is dropped per the Critical item).
