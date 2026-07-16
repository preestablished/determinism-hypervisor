# Positive Notes

## P1 — The load-bearing design choice is correct and explicitly justified

Seeding `F` from the device (`mov r10d, [r8 + REG_FRAME]`) instead of
`xor r10d, r10d` like `pad_echo` is exactly right. Because `FRAME_COUNTER` is
lineage-ABSOLUTE and restored from the PADD section (`pad.rs:15-19,152`),
reading it at entry makes `FRAME_MARK` continuity hold *by construction* for
every composition — crucially including a fresh-booted guest running against a
restored device state, where a register-only approach would restart at 1 and
break the seam. The asm header (`fake_frames.asm:6-16`) lays this reasoning
out clearly, and the drift test enforces the read actually exists.

## P2 — Drift pin defends the property, not just the constants

The positional check at `elf_shape.rs:584-600` strips comments first
(`l.split(';').next()`), so the doc-header sentence describing the read cannot
satisfy the pin — only the real instruction does. It then asserts
`read_at < loop_at`. This is a thoughtful, non-trivial guard: it pins the
*behavior* (read before bump), which is the actual load-bearing invariant,
rather than a brittle line number or a string that comments could spoof.

## P3 — Pace-loop equivalence is preserved and pinned

The `.pace` body — including the `0x9AD5` seed and the exact 7-instruction
sequence — is reproduced identically from `pad_echo` so both guests share the
same per-frame icount cadence. The test pins `FAKE_FRAMES_PACE_ITERS ==
PAD_ECHO_PACE_ITERS` (`elf_shape.rs:571`) and counts the body to exactly 7
(`elf_shape.rs:603-619`), so a future edit that drifts one guest's pacing from
the other fails CI.

## P4 — Disciplined minimal scope

The guest does exactly one job (make frames) and nothing else — no IDT, no
STI, no pad polling, no RAM table — which keeps it orthogonal to `pad_echo`
(the pad-input guest) and keeps the `FRAME_MARK` table the sole observable.
The supporting Rust changes are the minimum needed: one `PROGRAMS` entry, one
accessor, two well-documented constants, and registration in both the
shape sweep and the empty-check. No churn, no unrelated edits.

## P5 — Constants mirror device truth, not magic numbers

`FAKE_FRAMES_PACE_ITERS` and the asm `%define`s are cross-checked against
`dh_devices::pad::{PV_PAD_BASE, REG_FRAME_COUNTER}` and against
`PAD_ECHO_PACE_ITERS` in the drift test (`elf_shape.rs:569-571`), so the guest
can never silently fall out of sync with the device register map or its
sibling's cadence.
