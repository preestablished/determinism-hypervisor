# Positive Notes

- **Offsets are exactly right.** Every legacy offset in `xsave.rs:67-72` matches the SDM FXSAVE/XSAVE layout to the byte, including the subtle disjoint x87 union `[0,24) ∪ [32,160)` that deliberately steps over MXCSR. Getting the gap right (rather than the tempting-but-wrong `[0,160)`) is the single highest-risk detail in this change and it's correct.

- **The MXCSR decision is reasoned, not accidental.** The doc comment at `xsave.rs:33-41` and `52-58` correctly identifies that MXCSR is governed by RFBM's SSE/AVX bits, not `XSTATE_BV[1]`, and ties it to the concrete iteration-51 measurement (`hash.rs:261-266`). This is exactly the kind of architectural subtlety that silently rots a determinism boundary, and it's documented at the decision site.

- **Pure transform, correctly gated.** Keeping `canonicalize`/`xstate_bv` ungated (so they build and unit-test on aarch64) while x86-gating only `host_component_layout`/`unsafe_cpuid` is the right factoring — it lets the R7 logic be tested without KVM and keeps the CI matrix honest. `lib.rs:12-14` documents the split.

- **The R7 fault-injection test genuinely demonstrates the hazard.** `r7_uncanonicalized_garbage_changes_the_hash_canonical_does_not` (`xsave.rs:201-221`) holds *identical* live state (same set-bit content + MXCSR) in two buffers, injects *different* garbage into the clear x87 component, asserts the raw hashes differ (the fault the risk register warns about), then asserts canonicalization makes them byte-identical and equal-hashing. That's the actual R7 shape, not a tautology — it would fail if the transform were a no-op or zeroed the wrong range.

- **Errors are loud and bounded.** `TooShort` and `ComponentOutOfBounds` (`xsave.rs:43-49`) with checked-arithmetic bounds (`checked_add(...).filter(...)`) mean a malformed area or host table fails as a typed error rather than panicking or silently zeroing out-of-area bytes — important since 55f will feed this real SET-bound data. `bounds_are_loud` covers both.

- **Idempotence is tested** (`canonicalization_is_idempotent`), which matters for a transform that may be applied on both capture and restore paths.

- **Live stability test is the right one.** `live_xsave_canonicalizes_and_is_stable` reads `KVM_GET_XSAVE` twice with no guest execution between and asserts byte-identical canonical output — directly validating the GET-side stability premise the whole R7 fix rests on. Passed on this host.

- **No fixture churn introduced.** Changing the hash preimage is a quietly dangerous act in a project full of frozen BLAKE3 anchors; this change correctly lands without touching any pinned constant because the vCPU-blob path was never pinned to an absolute value (only relative properties). The author's comment chain shows awareness that the preimage moved.
