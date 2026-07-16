# Code Review — Overview

- **Branch:** `ralph/iteration-27-nanokernel-device-exercise` vs `main`
- **Bead:** determinism-hypervisor-7ys (M1-acceptance guest program)
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus
- **Commit reviewed:** `fbc61e4` — "ralph: iteration 27 checkpoint - nanokernel device-exercise guest (M1 acceptance program)"

## Scope

New M1-acceptance guest `tests/nanokernel/asm/device_exercise.asm` (246 lines)
exercising every pv device (clock, entropy, pad, blk) and the detchannel
(CHANNEL_INIT + one ring-W Beacon + doorbell), reporting one serial progress
byte per stage. Plus `build.rs` (program registration), `lib.rs` (ELF accessor
+ constants), `elf_shape.rs` (shape coverage).

## Files changed

| File | +/− | Notes |
|---|---|---|
| `tests/nanokernel/asm/device_exercise.asm` | +246 | new guest program |
| `tests/nanokernel/build.rs` | +1/−1 | `PROGRAMS += device_exercise` |
| `tests/nanokernel/src/lib.rs` | +17 | `device_exercise_elf()`, `DEVICE_EXERCISE_OK_SEQUENCE`, `_CHANNEL_GPA`, `_BEACON_ID` |
| `tests/nanokernel/tests/elf_shape.rs` | +1 | shape assertion |

## Verification performed

- `cargo test -p nanokernel` — **6 tests pass** (3 lib + 3 elf_shape).
- Disassembled the produced ELF (`objdump -d -M intel`) and verified every MMIO
  operand width, PIO port/width, immediate, and BootInfo access encoding.
- Cross-checked every device register offset/status constant against the live
  models: `crates/dh-devices/src/{clock,entropy,pad,blk}.rs`.
- Cross-checked the channel header layout, ring descriptors, record framing, and
  detcall ABI against the authoritative clean-room crates
  `../guest-sdk/crates/detguest-{wire,host}` AND the host-side
  `crates/dh-devices/src/detchannel.rs` attach/drain path.

## Summary

The four pv-device stages (clock/entropy/pad/blk) are **correct**: every register
offset, access width, status code, and control-flow check matches the live device
models, and the disassembly confirms the intended encodings. The asm is clean,
well-commented, and the failure-path letter mapping is right.

However, the **detchannel stage ('D') can never pass**: the asm writes ring W
descriptor `size = 0x1E0000`, which is not a power of two. The authoritative host
attach path (`detguest_host::Channel::attach`, consumed by
`dh-devices/src/detchannel.rs::channel_init`) rejects any non-power-of-two ring
size, so `CHANNEL_INIT` returns status 2 (BadMagicVersion), the guest's
`IN 0xD37C` reads nonzero, and it parks with lowercase `d`. The documented
"CEPBDX" full-success sequence is therefore **unreachable on real hardware**.

The root cause is a self-contradiction in the spec the asm faithfully transcribed:
ARCHITECTURE.md §2's layout table lists ring W at `0x1E0000`, but the same
section's normative power-of-two index discipline — and the implementation —
size ring W at **`0x10_0000` (1 MiB)**. The implementation wins; the asm followed
the wrong line of the doc.

This is execution-gated (no end-to-end VMM+serial test runs this guest yet), so it
does not break the build, but it defeats the program's entire purpose. A
host-runnable test over `MockGuestMem` + `Channel::attach` would catch it
immediately and is a clean coverage gap to close.

## Verdict

**Request changes.** One critical correctness bug (ring-W size not a power of two
⇒ stage D can never pass ⇒ OK sequence unreachable). The pv-device stages and all
other asm details are sound. Fix is a one-line descriptor change plus an optional
host-runnable regression test.

## Stats

- Critical: 1
- Important: 1
- Suggestions: 4
- Positive notes: 7
