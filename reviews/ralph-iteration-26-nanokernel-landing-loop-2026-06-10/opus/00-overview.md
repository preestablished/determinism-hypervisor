# Review Overview — landing_loop nanokernel guest

- **Branch:** `ralph/iteration-26-nanokernel-landing-loop` vs `main`
- **Bead:** determinism-hypervisor-7yr
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus
- **Commit:** c0774df (iteration 26 checkpoint)

## Summary

The change adds `landing_loop`, the deterministic long-runner guest for the M2
landing test and the M3 1e9-instruction determinism regression. The new
`tests/nanokernel/asm/landing_loop.asm` parses the BootInfo cmdline's leading
ASCII decimal digits as an iteration count (defaulting to 12,500,000), then runs
an LCG accumulator loop whose body is exactly 8 instructions per iteration,
storing each result through a 64 KiB `.bss` ring buffer before emitting `'L'` on
the debug serial port and returning to crt0's HLT park. Supporting plumbing:
`build.rs` registers the new program, `src/lib.rs` adds `landing_loop_elf()` plus
the harness-facing constants `LANDING_LOOP_INSTRS_PER_ITER = 8` and
`LANDING_LOOP_DEFAULT_ITERS = 12_500_000` (with a unit test pinning their product
to 100M), and `tests/elf_shape.rs` is generalized to shape-check every guest.

## What I verified empirically

I assembled and linked the guest with the repo's own toolchain (`nasm -f elf64`,
`ld -m elf_x86_64`, the project `link.ld`) and inspected the result:

- **Loop body is exactly 8 instructions.** Disassembly of `.loop`
  (`0x1000b0`–`0x1000ce`): `imul rax,r10` / `add rax,r11` / `rol rax,0xd` /
  `mov [r12+rdx*8],rax` / `add rdx,0x1` / `and rdx,0x1fff` / `sub rcx,0x1` /
  `jne .loop` — one instruction each, no surprises.
- **`align 16` pads the prologue, not the loop.** The 13 NOPs land between
  `xor edx,edx` and the `.loop` label (`0x1000a3`–`0x1000af`); they execute once,
  outside any counted iteration.
- **`.bss` layout has no overlap.** `BOOT_INFO_PTR` @ `0x100100`, stack
  `0x100110`→`0x104110` (16 KiB, grows down), `ring_buf` @ `0x104140` spanning
  64 KiB to `0x114140`. Separate object files concatenated by the linker; the
  reasoning in the prompt holds.
- **Mask consistency:** `and rdx,0x1fff` = 8191 = `BUF_QWORD_MASK`, and
  `resq 8192` = 64 KiB. Index cycles `0..8191`, so any N ≥ 8192 (the default is
  12.5M) touches the entire buffer.
- **All 5 nanokernel tests pass** (`cargo test -p nanokernel`): 3 unit + 2
  integration, including `default_iters_hit_the_100m_budget` and the generalized
  shape test.

## Verdict

**APPROVE.** The load-bearing artifact is correct and the determinism property
the bead asks for holds. No Critical or blocking issues. The findings below are
Important-tier documentation/robustness gaps (drift coverage for the two new
constants; the doc comment under-warns that the calibration offset varies with
cmdline length; an undocumented callee-saved-register clobber) plus minor
suggestions. None block merge.

## Stats

- Files changed: 4 (1 new asm, 3 modified)
- Lines: ~216 (diff)
- New tests: 2 (`default_iters_hit_the_100m_budget`, generalized
  `every_guest_is_a_static_x86_64_exec_at_the_load_addr`)
- Critical: 0 · Important: 3 · Suggestions: 4
