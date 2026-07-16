# Positive Notes

### The `is_x87_init` pattern is exactly the SDM architectural init state — verified byte-by-byte

`crates/dh-vmm/src/xsave.rs:143-147`. Mapping the FXSAVE legacy-area layout against the check:

| Offset | Field | Init value (SDM, FINIT) | Check |
|--------|-------|--------------------------|-------|
| [0,2)  | FCW   | 0x037F                   | `area[0..2] == [0x7F, 0x03]` (LE) ✓ |
| [2,4)  | FSW   | 0x0000                   | covered by `area[2..24].all(0)` ✓ |
| [4,5)  | FTW (abridged) | 0x00 = all-empty | covered by `area[2..24].all(0)` ✓ |
| [5,6)  | reserved | 0 | covered ✓ |
| [6,8)  | FOP   | 0 | covered ✓ |
| [8,16) | FIP   | 0 | covered ✓ |
| [16,24)| FDP   | 0 | covered ✓ |
| [24,32)| MXCSR/MASK | (not x87) | **correctly excluded** ✓ |
| [32,160)| ST0-7 | 0 | `area[32..160].all(0)` ✓ |

The abridged-FTW init value is the easy place to get this wrong — the *full* FTW init is 0xFFFF
(all tags = empty), but in the FXSAVE *abridged* byte each bit is 0 for an empty register, so
abridged init = 0x00. The code uses the abridged byte and checks for 0x00. **Correct.** FCW=0x037F
is also correct (RC=00 round-nearest, PC=11 extended, all exceptions masked). Excluding MXCSR from
the x87-init test is correct because MXCSR is SSE state, not x87, and is governed separately.

### The allowlist rewrite is the right structural fix, not a patch

The subtractive rule (zero clear-bit areas) could only ever zero bytes it explicitly named, so it
was *correct by enumeration* — and the enumeration silently missed `[416,512)`, `[528,576)`,
inter-component gaps, and the tail. The allowlist inverts the default to **zero unless kept**, so
no region can leak by omission ever again. This is the correct response to "a garbage region we
forgot to name" — you cannot forget to name a region in a deny-by-default design. The test
`non_component_garbage_is_always_zeroed` (line 262) pins exactly this across all four legacy-bit
combinations.

### MXCSR restore-safety is handled correctly (even if undocumented — see I2)

MXCSR/MXCSR_MASK `[24,32)` is unconditionally in the keep list (line 107) and never gated on a
bit. This is exactly what SET_XSAVE/XRSTOR needs: MXCSR loads from the area whenever RFBM[1]|[2]
is set regardless of XSTATE_BV[1], so clearing the SSE init-bit does not disturb restored MXCSR.
The prompt's specific corruption worry does not materialize, precisely because of this choice.

### Test quality is high and pins both the fix and its boundaries

- `init_state_normalizes_to_clear_bit_regardless_of_encoding` (line 212) pins the *core invariant*:
  both KVM encodings (bit-clear-garbage and bit-set-init-pattern) → byte-identical canonical form,
  for x87, SSE, AND extended — and pins that **non-init x87 keeps its bit** (the corruption guard).
- `bounds_are_loud` now checks OOB is loud **whether the component bit is clear or set** (line 324)
  — the allowlist refactor changed the bounds-check ordering (it now always computes `end`), and
  this test correctly pins that the set-bit path still errors. Good defensive coverage of the
  refactor's actual behavior change.
- `canonicalization_is_idempotent` (line 357) guards against the rewrite ever producing a
  non-fixed-point output — important now that the function rewrites XSTATE_BV in place.
- The live KVM test `live_xsave_canonicalizes_and_is_stable` passes on this host, exercising the
  real `GET_XSAVE` → canonicalize → re-read → identical path.

### The commit message records the process lesson

The commit (`920069f`) explicitly states iteration 68 "shipped the subtractive rule and its
verification missed (the failure needs parallel-suite host load)" and that the verification gap
was the missing parallel-load condition. That is the correct lesson captured at the right altitude
— the bug was not the subtractive rule being *wrong* in isolation, it was the gate not reproducing
the host-preemption race. No additional lesson file is needed; the commit body is the right home.

### Verification ran green

94/94 `dh-vmm` lib tests (including the live KVM test) and `skid_gate` 2/2 across 4 consecutive
runs in this review. No flakes observed.
