# Review 01 — Critical & Important Findings

**None.** No Critical and no Important findings. The hotfix is correct and merge-ready.

I specifically tried to break it and could not:

- **Allowlist completeness** — the rebuild starts from an all-zero buffer and copies back only allowlisted ranges, so it is structurally impossible to leak a garbage region by omission (the failure mode of the iteration-68 subtractive rule). Verified by `non_component_garbage_is_always_zeroed` and by reasoning against this host's real layout (gap `[832,960)`, legacy-reserved `[416,512)`, header-reserved `[528,576)`, tail) — all zeroed.
- **Init normalization is sound and deterministic** — `is_x87_init` and the all-zero checks are pure functions of the input bytes; the normalization decision does not depend on host state, so it cannot itself introduce nondeterminism. Both KVM encodings of logically-init state map to the same canonical bytes (`init_state_normalizes_to_clear_bit_regardless_of_encoding`).
- **Bounds are loud on set bits too** — the iteration-68 bug class (OOB only checked on clear bits) is fixed: the bounds check now runs before the bit test, asserted by the extended `bounds_are_loud` for both clear and set bits.
- **`is_x87_init` matches the SDM** (Vol.1 §10.5.1 / Vol.2 FXSAVE layout): FCW=0x037F (`[0,2)==[0x7F,0x03]` LE ✓), FSW=0, abridged FTW=0x00 (abridged tag: 1=valid/0=empty, all-empty init = 0x00 ✓), FOP/FIP/FDP=0, ST0..7=0 — all covered by `[2,24)==0 && [32,160)==0`. MXCSR `[24,32)` correctly excluded (not x87 state). **No byte inside `[0,24)∪[32,160)` is missed.** The live test `live_xsave_canonicalizes_and_is_stable` passing on real KVM output empirically confirms the abridged-FTW=0x00 assumption holds on this box.
- **MXCSR restore correctness (55f future path)** — confirmed against the SDM XRSTOR pseudocode: when `RFBM[1]=1` (restore mask covers SSE), **MXCSR is loaded from the memory image regardless of XSTATE_BV[1]**. So normalizing the SSE bit to clear while keeping the MXCSR bytes in the canonical area makes a future XRSTOR exact — XMM re-zeroes from the init path, MXCSR loads from the bytes we preserved. **No corruption.** Keeping MXCSR is the correct choice.

The one substantive observation — that the *stated* dual-encoding mechanism did not reproduce on this box — is recorded as a Suggestion (doc precision), not an Important finding, because it does not affect correctness: the fix closes both the non-component-garbage hole (demonstrably the flake) and the init-encoding ambiguity (whether or not it manifests here).
