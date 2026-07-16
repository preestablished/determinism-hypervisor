# Review 03 — Positive Notes

## P1 — Allowlist is the structurally correct shape
Rebuilding from an all-zero buffer and copying back only allowlisted ranges makes "leak a garbage region by omission" structurally impossible. This is the right inversion of the iteration-68 subtractive rule, which could only ever zero the regions someone remembered to enumerate. The §8.1 goal ("blob equality ⇔ logical-state equality") is actually delivered now, not approximated.

## P2 — Bounds check moved before the bit test
Extended-component OOB is now caught whether the bit is clear *or* set (`xsave.rs:117-121`), closing the iteration-68 gap where a set-bit OOB slipped past. The new assertion in `bounds_are_loud` pins it.

## P3 — Init-normalization is a pure, deterministic transform
The normalization decision (`is_x87_init`, all-zero area checks) depends only on the input bytes — never on host/timing state — so it cannot itself introduce nondeterminism. That is the essential property for anything feeding a determinism hash, and the design has it.

## P4 — Tests pin both failure classes explicitly
`init_state_normalizes_to_clear_bit_regardless_of_encoding` (both encodings → identical bytes, across x87/SSE/extended) and `non_component_garbage_is_always_zeroed` (reserved/header-reserved/tail zeroed for every bv) directly encode the two hypothesized root causes. `r7_uncanonicalized_garbage_changes_the_hash_canonical_does_not` and `canonicalization_is_idempotent` round it out. Good regression coverage.

## P5 — Live test asserts stability against real KVM
`live_xsave_canonicalizes_and_is_stable` reads `GET_XSAVE` twice and asserts byte-identical canonical output — the right shape for catching real kernel variance, and it passed here. It also empirically validates the abridged-FTW=0x00 and "MXCSR always populated" assumptions on real output.

## P6 — Honest, specific comments tied to measured evidence
The doc references the actual iteration-68 measurement, names the exact byte ranges at risk, and explains *why* MXCSR is kept (always-live, not XSTATE_BV-governed; two-byte duplication with the pinned MXCSR field is called out as deliberate belt-and-braces in `hash.rs:274-276`). This is the kind of provenance that makes a determinism codebase auditable.

## P7 — MXCSR sourcing is correct end-to-end
`hash.rs:261-268` sources MXCSR from `GET_XSAVE` (region[6]) rather than the lying `kvm_fpu.mxcsr`, and the canonical area keeps `[24,32)` — consistent with the SDM XRSTOR rule that MXCSR loads from memory when RFBM covers SSE/AVX. The future restore path will be exact.
