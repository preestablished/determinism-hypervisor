# Critical & Important Findings

**None.** No Critical or Important issues were found.

The change is correct on every load-bearing axis I checked. The reasoning behind that conclusion is recorded here so a future reviewer (and bead 55f, which reuses `canonicalize` for `KVM_SET_XSAVE`) can rely on it.

## SDM offset verification (the Critical-if-wrong axis) — all correct

Against the Intel SDM XSAVE chapter (legacy region = FXSAVE area layout):

| Field | Offset | In code | SDM |
|---|---|---|---|
| FCW | [0,2) | part of `[0,24)` x87 fill | ✓ |
| FSW | [2,4) | ✓ | ✓ |
| FTW (abridged) | [4,5) | ✓ | ✓ |
| reserved | [5,6) | ✓ | ✓ |
| FOP | [6,8) | ✓ | ✓ |
| FIP | [8,16) | ✓ | ✓ |
| FDP | [16,24) | ✓ | ✓ |
| **MXCSR** | [24,28) | **NOT zeroed** | ✓ |
| **MXCSR_MASK** | [28,32) | **NOT zeroed** | ✓ |
| ST0–ST7 (MM0–7) | [32,160) = 8×16 | x87 fill | ✓ |
| XMM0–XMM15 | [160,416) = 16×16 | SSE fill | ✓ |
| reserved/sw-avail | [416,512) | untouched | ✓ |
| XSTATE_BV | [512,520) | read | ✓ |
| XCOMP_BV | [520,528) | (std format, 0) | ✓ |

`xsave.rs:67-72` (`area[0..24].fill(0)`, `area[32..160].fill(0)`, `area[160..416].fill(0)`) matches the table exactly. The x87 component is the disjoint union `[0,24) ∪ [32,160)` — the gap is precisely MXCSR — and the code skips that gap. No offset error → no corruption of the hash preimage, and (important for 55f) no corruption of real state if these same fills are fed to `KVM_SET_XSAVE`.

## MXCSR "do not zero" — sound for GET-side hash stability

The question is whether MXCSR at `[24,32)` is byte-stable out of `KVM_GET_XSAVE` even when `XSTATE_BV[1]` (SSE) is clear, because if it varied, leaving it un-zeroed would be a residual R7 hole.

- MXCSR/MXCSR_MASK are **not** governed by an `XSTATE_BV` bit. In `XSAVE`/`XRSTOR` they are written/read whenever **RFBM[1] (SSE) or RFBM[2] (AVX)** is set in the operation's request mask — independent of the *saved* `XSTATE_BV[1]`. The init optimization that produces R7 applies to *component areas* keyed by `XSTATE_BV`; MXCSR is not such an area.
- `KVM_GET_XSAVE` issues `XSAVE` with a full request mask (guest XCR0), so MXCSR is always written to the buffer with the live, valid value regardless of whether SSE state happens to be in init. The in-tree comment ("always valid in KVM output", `xsave.rs:36-37`, and the iteration-51 finding referenced at `hash.rs:261-266`) is accurate.
- On the SET side (55f): `XRSTOR` with RFBM[1]=0 ignores the MXCSR field in the buffer, so a preserved (non-zeroed) MXCSR is harmless to restore. With RFBM[1]=1 it is restored — which is the correct/desired behavior since it is real state.

Conclusion: not zeroing MXCSR is the correct decision and is **not** a residual R7 hole. The hash already carries MXCSR twice (once as the pinned `region[6]` field at `hash.rs:268`, once inside the canonical area) — see Suggestions for the (benign) double-count note.

## Extended-table / CPUID semantics — correct

`host_component_layout()` (`xsave.rs:101-122`):
- Subleaf 0: `supported = EAX | (EDX << 32)` — correct; leaf 0xD subleaf 0 EAX:EDX is the XCR0-valid bitmap.
- Per supported bit ≥ 2: subleaf = bit, `size = EAX`, `offset = EBX` — correct register assignment for the **standard** format (which is what KVM returns; XCOMP_BV = 0).
- Uses host *supported* bits, not guest *enabled* XCR0. This is safe and arguably preferable: in Phase 1 the guest CPUID masks everything past SSE and OSXSAVE is off, so `XSTATE_BV` bits ≥ 2 are never set and those component areas never need zeroing anyway; the loop simply iterates a few extra entries that all hit the `bv & (1<<bit) == 0 → fill(0)` of already-init areas. No correctness impact, and it future-proofs the table if the guest XCR0 is later widened. Bounds are checked (`ComponentOutOfBounds`), so a host table that overruns a (hypothetically short) buffer fails loud rather than corrupting.

## Feeding canonicalized data to KVM_SET_XSAVE (55f safety) — safe

`XRSTOR` (the SET path) restores a component's registers from its area **only when both** `RFBM[i]=1` **and** `XSTATE_BV[i]=1`; when `XSTATE_BV[i]=0` it loads the component's *init* values and **ignores the area bytes entirely**. Zeroing a clear component's area therefore cannot change restored state — the zeros are never read. So `canonicalize` output is safe to round-trip through `KVM_SET_XSAVE`. (The one nuance for 55f to preserve: XSTATE_BV/XCOMP_BV header bytes and MXCSR must not be clobbered — and `canonicalize` leaves all three intact.)

## Hash-preimage churn — nothing pins absolute values

Changing `canonical_vcpu_blob` changes every state hash. I searched for pinned chain/golden constants:
- `crates/dh-snapshot/tests/golden.rs` and `crates/dh-inputlog/tests/golden.rs` pin BLAKE3 of **checked-in fixture files** (format-freeze anchors), not the vCPU blob.
- `crates/dh-snapshot/tests/dhsnap_codec.rs:12` uses a synthetic `[0x22; 200]` placeholder for the VCPU section — not the real blob.
- The `dh-vmm` hash tests (`hash.rs:526-555`) assert **relative** properties (read-stability, TSC-slot sensitivity, RAM-flip sensitivity), never an absolute value.

No fixture or constant needs regeneration; nothing breaks.
