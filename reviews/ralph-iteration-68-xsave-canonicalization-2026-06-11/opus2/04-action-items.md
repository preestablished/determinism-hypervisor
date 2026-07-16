# Action Items

## Critical

- [ ] None. No blocking issues found. Branch is mergeable as-is.

## Important

- [ ] **File a bead-55f note: `KVM_GET_XSAVE` (fixed 4096 B) truncates on
      AVX-512 + AMX hosts.** `crates/dh-vmm/src/hash.rs:267` uses `get_xsave()`
      whose `kvm_xsave.region` is `[u32; 1024]` = 4096 bytes. `kvm-ioctls 0.24`
      already exposes `get_xsave2()` (`KVM_GET_XSAVE2`). On a host whose XSAVE
      area > 4096 B (AMX XTILEDATA alone is 8 KiB), the captured area would be
      truncated and the hash preimage incomplete. Not live on the lab box
      (this host: XCR0=0x1f → 1088 B; `KVM_CAP_XSAVE2` reports 4096). Fails
      *closed* today because `host_component_layout()` would enumerate an
      offset ≥ 4096 and `canonicalize` returns `ComponentOutOfBounds`. In 55f:
      either move to `get_xsave2()` sized from `KVM_CAP_XSAVE2`, or add an
      explicit preflight guard:
      ```rust
      let xsave2 = kvm.check_extension_int(Cap::Xsave2);
      if xsave2 > 4096 { return Err(KvmError::Open(
          format!("host XSAVE area {xsave2} B exceeds fixed KVM_GET_XSAVE; use XSAVE2 (55f)"))); }
      ```
      Record the live measurements from this review (XCR0=0x1f, area 1088,
      CAP_XSAVE2=4096) in the bead.

## Suggestions

- [ ] **S-1:** Strengthen `live_xsave_canonicalizes_and_is_stable` to assert the
      transform *changed* bytes when a clear component held nonzero data (proven
      live: fresh vCPU has `FCW=0x037f` in the clear x87 area). Snapshot pre-bytes
      and `assert_ne!(pre, post)`. `crates/dh-vmm/src/xsave.rs` (live_tests).
- [ ] **S-2:** Add one sentence to the `canonicalize` doc clarifying that clear
      components are zeroed (NOT set to architectural init values like FCW=0x037f)
      because this is a hash preimage, never a restored area.
      `crates/dh-vmm/src/xsave.rs`.
- [ ] **S-3:** Cache `host_component_layout()` in a `OnceLock` (host-invariant) to
      avoid re-running CPUID 0xD on every `canonical_vcpu_blob` call.
      `crates/dh-vmm/src/xsave.rs` + `crates/dh-vmm/src/hash.rs:278`.

## Verification performed (this review, on real x86_64 + /dev/kvm)

- [x] Host CPUID 0xD layout printed and cross-checked (AVX 576/256 ✓).
- [x] Live fresh-vCPU XSTATE_BV + clear-component garbage probe (R7 live ✓).
- [x] `KVM_CAP_XSAVE2` size measured (4096 ✓, fits).
- [x] Two-read XSAVE stability confirmed (byte-identical ✓).
- [x] No pinned state-hash/chain value broken by the preimage change ✓.
- [x] `cargo check -p dh-vmm --target aarch64-unknown-linux-gnu` ✓.
- [x] `cargo test -p dh-vmm --lib` — 91 passed (incl. live xsave) ✓.
- [x] `cargo test -p determinism-tests --test regression` — 2 passed ✓.
- [x] `cargo clippy -p dh-vmm --lib` clean ✓.
