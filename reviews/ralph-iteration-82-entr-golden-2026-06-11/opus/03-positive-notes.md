# Positive notes

## The acceptance is honest and hard to fake

The strongest thing about this test is that it has **no false-pass path**. Leg B
resumes at `.batch`, *after* the one-time `LEN` programming in `prog_main`, so
the restored device's `LEN` register is genuinely load-bearing for byte
equality — not just snapshotted-and-ignored. Both ENTR v2 halves (PRNG state +
device regs) must round-trip exactly or the byte assertion fails. The
"un-snapshotted continuation IS the golden" framing is the right way to build a
restore-fidelity test: it compares the restored machine against a live machine
that never paused, so any divergence introduced by snapshot/restore shows up as
a byte difference. The triple assertion (ring bytes equal + `DetEntropy::state()`
equal + exact count pins) closes prefix-equality and final-position gaps.

## The HLT-batch design correctly dodges the landing hazard

Rather than poll for a goal landing inside MMIO-dense code (which overshot —
bead 4a3), the guest HLTs every 256 draws and each segment stops on the exact,
zero-skid `GuestHalted` boundary. This is the right call: it makes the
acceptance independent of the single-step-across-MMIO landing problem entirely,
and it does so using only mechanisms that already existed (terminal-HLT handling
+ same-slot segment re-entry). The module docs in both the guest and the test
explain the reasoning clearly, and the failure they hit was filed as its own
bead with a concrete repro instead of being papered over.

## The drift pins are thorough and use device-side truth

`elf_shape.rs::entropy_draw_asm_matches_rust_constants` doesn't just pin the
guest's `%define`s against `nanokernel` consts — it pins the MMIO register
offsets and `STATUS_OK` against `dh_devices::entropy`'s own `pub const`s, so a
register-map change on the device side fails the guest drift test loudly. The
shape pin (`assert_guest_shape`) is also wired in. This is exactly the right
layering: the ABI between guest and device is checked from the device's
authoritative constants.

## Faithful clone of the m1 on_exit pattern

`run_one_batch`'s `on_exit` mirrors `m1_acceptance`'s servicing loop precisely:
`VmMem` adapter, fresh `DevCtx` per dispatch, `log_fault()` check after each
exit, and an undrained-irq assertion (`assert!(irqs.is_empty())`). The
`assert_eq!(out.reason, StopReason::GuestHalted, "expected the exact
batch-boundary HLT")` makes the zero-skid contract explicit per batch. Reusing
the proven m1 fidelity pattern rather than inventing a new servicing loop is the
right move.

## Correct treatment of the continuous counter axis

The test shares one `InstRetired` counter across both legs (same thread, never
reset) and is explicit — in the module doc and at `restore_snapshot(.., None,
None, ..)` — that chain values are deliberately **not** compared across legs
because absolute icounts differ by construction. The byte equality and final
PRNG state are the claim, and those are axis-independent. This avoids the classic
false-failure of comparing icount-derived chain links across a restore.

## Ring layout is host-friendly and bounded

The monotone u64 count header is bumped **after** the device writes the slot
(torn-read discipline for a host sampling mid-run), the ring is sized
(2^15 slots) far beyond any run here so no wrap-eviction is possible, and the
`read_draws` helper's `& (CAPACITY-1)` masking is correct. The "ring ends at
0x580008" comment checks out arithmetically.
