# Action Items

### Critical

_None._

### Important

_None._ (See `01-critical-and-important.md` for the issues hunted and cleared, and why the
accounting findings landed as Suggestions rather than escalating.)

### Suggestions

- **[S1] Make the 13 retiring `align 16` NOPs visible.** The `align 16` before `.loop` in
  `tests/nanokernel/asm/landing_loop.asm` pads `0x100a3..0x100af` with 13 single-byte NOPs
  that retire on `INST_RETIRED.ANY` and are part of the fixed prologue offset. Either add a
  comment at the `align 16` line noting "pads ~13 RETIRING NOPs" or drop `align 16` (the
  imul/rol-bound 32-byte loop gains little from entry alignment) so the prologue count matches
  the source line-for-line. Self-contained; no behavior change required either way.

- **[S2] Clarify in `tests/nanokernel/src/lib.rs` that the prologue offset is per-cmdline.**
  The `LANDING_LOOP_INSTRS_PER_ITER` doc's "calibrate the exact offset once, it is
  deterministic" is true only for a fixed cmdline: each extra cmdline decimal digit runs the
  parse loop body once more (~9–10 retired instructions/digit), and the valid-BootInfo path
  differs from the empty-cmdline default path. Reword to "deterministic for a given cmdline;
  recalibrate if the cmdline digit count changes" so a harness doesn't reuse the empty-cmdline
  offset against a digit-bearing run.

- **[S3] Add a host-side guard that pins the loop body to 8 instructions.** No test fails today
  if `.loop` is edited to 7 or 9 instructions while `LANDING_LOOP_INSTRS_PER_ITER` stays 8 —
  the worst silent drift for an icount harness. Cheapest version: in an `elf_shape`-style test,
  find the `prog_main.loop` symbol and assert the byte span to its terminating back-`jne`
  equals the known body length (32 bytes in the current build), or decode that span and assert
  it contains exactly `LANDING_LOOP_INSTRS_PER_ITER` instructions. Dependency-free; uses the
  ELF symbol table already present.

- **[S4] Comment the parser's intentional overflow behavior in `landing_loop.asm`.**
  `imul rax,rax,10` wraps silently on a >64-bit cmdline value; the result is deterministic and
  accepted as-is. Note that overflow is wrap-defined (and benignly falls back to DEFAULT only
  if it happens to wrap to 0). Optionally cap accepted digits (~10–12) so an operator typo
  can't request ~1e18 iterations and hang the landing test. Low priority — cmdline is
  operator-controlled.

- **[S5] Document the `r12` callee-saved clobber in `landing_loop.asm`.** `prog_main` clobbers
  SysV callee-saved `r12` without restoring it. Safe today because crt0 only `call`s
  `prog_main` then parks in HLT, reading no saved registers — but a one-line comment
  ("clobbers r12; crt0 parks and reads no callee-saved regs") protects against future crt0
  changes that assume the callee-saved set survived.
