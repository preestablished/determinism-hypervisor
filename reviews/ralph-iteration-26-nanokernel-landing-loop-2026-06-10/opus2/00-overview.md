# Review Overview — iteration 26: nanokernel landing-loop

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-26-nanokernel-landing-loop` vs `main`
- **Bead:** 7yr
- **Angle:** x86-64 assembly correctness, instruction-count accounting, determinism hazards, harness-contract gaps

## What landed

A new long-runner guest for the M2 landing test / M3 1e9 determinism regression:

- `tests/nanokernel/asm/landing_loop.asm` — cmdline-parsed iteration count, an 8-instruction
  LCG loop body storing through a 64 KiB ring buffer, `'L'` to serial, then crt0 HLT park.
- `tests/nanokernel/src/lib.rs` — `landing_loop_elf()`, `LANDING_LOOP_INSTRS_PER_ITER=8`,
  `LANDING_LOOP_DEFAULT_ITERS=12_500_000`, plus a `default_iters_hit_the_100m_budget` test.
- `tests/nanokernel/build.rs` — `landing_loop` added to `PROGRAMS`.
- `tests/nanokernel/tests/elf_shape.rs` — `pipeline_smoke_is_a_static…` generalized into a
  reusable `assert_guest_shape(name, elf)` applied to both guests.

## Verification performed

- Built with `cargo test -p nanokernel`; **all 5 tests pass** (3 unit + 2 integration).
- Disassembled the built `landing_loop.elf` (`objdump -d -M intel`) and walked every
  instruction in the prologue, parse loop, setup, loop body, and epilogue.
- Confirmed symbol addresses (`objdump -t`), section/segment layout (`objdump -h`,
  `readelf -l`), and `lea` addressing modes from raw bytes.
- Simulated 16 iterations of the LCG+rol in Python (stream non-zero, non-repeating).
- Verified ring-buffer bounds, BSS non-overlap, and DEFAULT_ITERS encoding arithmetically.

## Verdict

**APPROVE.** The assembly is correct, the loop body is **exactly 8 retired instructions
per iteration** (disassembly-confirmed), the ring store is in-bounds, the BSS layout has no
overlap, and the LCG stream is meaningful. No correctness bugs.

The single substantive finding is an **accounting/documentation subtlety**, not a bug: the
`align 16` before `.loop` injects **13 retiring NOPs**, and the prologue retired-count varies
with the number of cmdline digits. Both are fully deterministic per-cmdline, so the
"calibrate the offset once" contract in `lib.rs` holds — but the doc undersells *why* the
offset is per-cmdline. I also recommend a host-side guard that pins the loop body to 8
instructions against silent edits (no test does this today).

## Stats

| Severity   | Count |
|------------|-------|
| Critical   | 0     |
| Important  | 0     |
| Suggestions| 5     |
| Positive   | 6     |

The Important list is intentionally empty: the prologue/NOP accounting items are real and
worth fixing in docs/tests, but none of them break determinism or correctness, so they land
as Suggestions. See `01-critical-and-important.md` for why each was *not* escalated.
