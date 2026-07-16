# Action Items

Branch: `ralph/iteration-69-xsave-allowlist-hotfix` — XSAVE allowlist + init-encoding hotfix.
Verdict: **APPROVE**. No Critical items. The two Important items are follow-ups that must land
**before** bead 55f reuses `canonicalize()` on the `KVM_SET_XSAVE` restore path; they do not block
this hash-only hotfix.

## Action Items

### Critical

- [ ] None.

### Important

- [ ] **I1 — Restrict (or gate) the generic extended-bit init normalization before 55f restore reuse.**
  `crates/dh-vmm/src/xsave.rs:122-128`. The comment `all-zero ⇒ init for bits ≥ 2` is not true for
  every XCR0 component; the generic byte-all-zero heuristic would wrongly clear a non-all-zero-init
  component's bit. On the hash path this is a (negligible) collision; on the future SET_XSAVE
  restore path it is **state corruption** (XRSTOR loads the component's init state instead of the
  guest's real state). Fix: either (A) restrict extended init-normalization to an explicit
  `ZERO_INIT_BITS` allowlist (currently just AVX bit 2, each addition citing the SDM init value),
  or (B) add a `normalize_init` flag / separate `canonicalize_for_restore` so 55f opts out until
  each component's init pattern is verified. File a bead and reference it in the function doc.
  (Safe today: Phase 1 masks all extended components, so the generic loop never receives one.)

- [ ] **I2 — Document the SET_XSAVE/MXCSR restore-safety contract on `canonicalize`.**
  `crates/dh-vmm/src/xsave.rs:58-81`. The code correctly keeps MXCSR `[24,32)` unconditionally, so
  clearing the SSE bit does NOT reset restored MXCSR (XRSTOR loads MXCSR from the area whenever
  RFBM[1]|[2] is set, independent of XSTATE_BV[1]). This invariant is currently implicit; add a
  `# Restore safety (55f)` doc paragraph stating: MXCSR is always kept and must never be moved
  under the SSE bit; init-bit clearing is restore-safe only for all-zero-init components (x87, SSE);
  see I1 before reusing the generic extended-bit path on restore. Documentation-only.

### Suggestions

- [ ] **S1** — Canonicalize XCOMP_BV to 0 in the rebuilt blob (it is not logical state in standard
  form), or assert it equals 0; at minimum comment the deliberate pass-through at
  `crates/dh-vmm/src/xsave.rs:136`.
- [ ] **S2** — Add `debug_assert!(area.len() >= XSAVE_MIN_LEN)` or a precondition doc line to
  `is_x87_init` (`xsave.rs:143`) so its indexing safety is local to the function.
- [ ] **S3** — (Only if hashing shows in a profile) avoid the per-call `keep` Vec + shadow-buffer
  allocation by zeroing non-kept ranges in place or using a fixed-size SmallVec. Do not do
  speculatively.
- [ ] **S4** — Add one assert that a set-bit *extended* component with a non-zero area keeps its
  bit (`assert_eq!(xstate_bv(...).unwrap(), 0b111)` in the AVX-set case at `xsave.rs:295`), pinning
  the extended init-normalization boundary as tightly as the x87 boundary is pinned.
