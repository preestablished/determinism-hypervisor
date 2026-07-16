# Review 04 — Action Items

## Action Items

### Critical
- [ ] None.

### Important
- [ ] None. The hotfix is correct and merge-ready.

### Suggestions
- [ ] **Delete the reviewer's scratch probe before merge.** `crates/dh-vmm/tests/xsave_dual_encoding_probe.rs` is a non-asserting reproduction probe I added (the interleaved variant takes ~66s). Remove it, or gate both tests behind `#[ignore]`, so it does not slow `cargo test --workspace`. It is not part of the hotfix.
- [ ] **Soften the dual-encoding doc comment to match evidence.** In `crates/dh-vmm/src/xsave.rs:73-77` and `:89-95`, the "KVM reports either encoding depending on preemption timing" claim did not reproduce on this box (kernel 6.8.0-124) across ~232k `GET_XSAVE` reads under heavy host-FPU load — bit0 stayed set every time. The directly-reproduced flake driver was the non-component-gap garbage (real gap `[832,960)` between AVX and BNDREGS on this host, plus reserved regions and tail). Reword the init-encoding rationale as "may vary across kernels" rather than measured-here; keep the normalization (correct, cheap) and keep the accurate iteration-68 measurement note at `:64-66`.
- [ ] **Normalize `XCOMP_BV` in the canonical header.** `crates/dh-vmm/src/xsave.rs:84-86, 136` passes `XCOMP_BV` through verbatim after only rejecting bit63. KVM standard form is always 0; assert `xcomp_bv == 0` (or write 0 into the canonical header) so the canonical form has a single header representation and no bit63-clear non-zero value can reach the hash preimage.
- [ ] **(Future, profiling-gated) Avoid per-call allocation in `canonicalize`.** It allocates a `keep` Vec and a full-size zero buffer every call. Fine at current hash rates; if hash frequency grows, zero the non-allowlisted ranges in place instead. No action now.
