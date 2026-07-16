# Action Items

Verdict: **APPROVE**. Nothing here blocks merge.

## Critical

- [ ] None.

## Important

- [ ] None.

## Suggestions

- [ ] **Guard against compact XSAVE format.** In `crates/dh-vmm/src/xsave.rs::canonicalize`, read XCOMP_BV at bytes `[520,528)` (already in-bounds given `XSAVE_MIN_LEN = 576`) and `debug_assert!` / reject when its compaction bit (MSB) is set, since all fixed offsets and the CPUID `EBX` offsets in the extended table are standard-format-only. Protects the bead-55f `KVM_SET_XSAVE` reuse from a silently-wrong area if a compact-format path (e.g. `KVM_GET_XSAVE2`) is ever wired in.

- [ ] **Strengthen `host_layout_is_sane`** in `crates/dh-vmm/src/xsave.rs` (live_tests) to also assert the CPUID-derived table entries do not overlap each other, catching a malformed host table near its source. Optional; `canonicalize` already bounds-checks against the real buffer.

- [ ] **Optional efficiency:** in `crates/dh-vmm/src/hash.rs:277`, the `region → Vec<u8>` round-trip allocates; if this blob is ever built per-instruction-boundary at high frequency, zero in place over a byte view of `xsave.region` to drop one allocation. No correctness impact.

- [ ] **Optional clarity:** add a one-line back-reference at `crates/dh-vmm/src/hash.rs:268` noting MXCSR (`region[6]`) is also covered by the canonical area appended below and is kept pinned per the iteration-51 finding, so the next reader doesn't have to cross-check the duplication.
