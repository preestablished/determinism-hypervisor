# Review 02 — Suggestions (non-blocking)

## S1 — Doc precision: the dual-encoding mechanism is asserted but not reproduced here

`crates/dh-vmm/src/xsave.rs:73-77, 89-95` state, as fact, that KVM reports init-state components "either way depending on host preemption timing." On this box (kernel 6.8.0-124) I could **not** reproduce the XSTATE_BV flip across ~232k `GET_XSAVE` reads in two configurations under heavy host-FPU contention — bit0 stayed set every time. The directly-demonstrable flake driver is the **non-component-gap garbage** the subtractive rule left untouched (this host has a real 128-byte gap `[832,960)` between AVX and BNDREGS, plus reserved regions and tail).

Suggestion: soften the comment to reflect evidence strength — e.g. "KVM *may* report init state either bit-clear or bit-set (encoding observed to vary across kernels; the non-component-gap garbage was the directly-reproduced driver of the iter-68 flake on 6.8.0-124)." Keep the normalization — it is correct and cheap insurance — just don't overstate the mechanism as measured-here when the measured-here flake was the gap garbage. The comment at lines 64-66 ("MEASURED: iteration 68 shipped the subtractive rule and ... flaked") is accurate and should stay.

## S2 — XCOMP_BV is passed through, not normalized

`crates/dh-vmm/src/xsave.rs:84-86, 136` reads `xcomp_bv`, rejects bit63 (compacted), and copies the value verbatim into the canonical header. KVM standard-format output always has `XCOMP_BV == 0`, so this is safe today. But a non-zero, bit63-clear `XCOMP_BV` (e.g. a future kernel or a malformed input) would pass through into the hash preimage unnormalized. Since the code already commits to standard-format-only semantics (`CompactedFormat` guard), consider asserting `xcomp_bv == 0` (or zeroing it in the canonical output), so the canonical form has exactly one representation of the header. Low risk; defense-in-depth for the hash preimage.

## S3 — `keep` Vec + full-area clone allocates on a hot path

`canonicalize` allocates a `Vec<(usize,usize)>` and a fresh `vec![0u8; area.len()]` every call. It runs once per state-hash (per boundary). Functionally fine and clearer than in-place surgery, but if hash frequency grows this is avoidable: zero the non-allowlisted ranges in place instead of rebuilding. Not worth doing now — correctness and readability win at current call rates — noting only for future profiling.

## S4 — Scratch probe test left in the tree (reviewer artifact)

I added `crates/dh-vmm/tests/xsave_dual_encoding_probe.rs` to attempt the reproduction. It is a non-asserting probe (~66s for the interleaved variant over 4000 fresh VMs) and is **not** part of the hotfix. It should be **deleted before merge** (or moved behind `#[ignore]`) so it does not slow the suite. I am leaving it for now so the next session can inspect the methodology; flagging here for cleanup.
