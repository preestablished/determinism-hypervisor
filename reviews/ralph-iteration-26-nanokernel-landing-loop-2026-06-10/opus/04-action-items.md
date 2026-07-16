# Action Items

Self-contained follow-ups. None block merge; I-1 and I-2 are the ones most worth
doing before this guest is wired into the M2/M3 harness.

### Critical

_None._

### Important

- **[I-1] Add asm↔Rust drift coverage for the landing-loop constants.**
  `LANDING_LOOP_INSTRS_PER_ITER` (=8) and `LANDING_LOOP_DEFAULT_ITERS`
  (=12,500,000) in `tests/nanokernel/src/lib.rs:49,53` restate values that are
  authoritatively defined in `tests/nanokernel/asm/landing_loop.asm` (the
  `%define DEFAULT_ITERS 12500000` at line 25, and the implicit 8-instruction
  loop body). They can drift silently and break the icount contract with zero
  test failures. Mirror the existing `bootinfo_inc_matches_rust_constants`
  pattern (`tests/nanokernel/tests/elf_shape.rs:64`): (a) parse the asm
  `%define DEFAULT_ITERS` and assert it equals `LANDING_LOOP_DEFAULT_ITERS`; and
  (b) add a guard that the `.loop:`…`jnz .loop` region in `landing_loop.asm`
  contains exactly `LANDING_LOOP_INSTRS_PER_ITER` instruction-bearing lines
  (ignoring comments, blanks, `align`, and labels).

- **[I-2] Fix the misleading "calibrate the offset once" doc.**
  `tests/nanokernel/src/lib.rs:45–48` tells harnesses to compute
  `8 * iters + prologue` and "calibrate the exact offset once." The prologue
  cost is NOT constant across cmdlines — the `.parse` loop runs once per cmdline
  digit, so totals differ by ~10 instructions per digit. Reword the doc to say
  the offset is deterministic *per cmdline* and grows with the number of parsed
  digits; recommend calibrating against the exact cmdline in use, or against the
  no-cmdline/empty-cmdline path (which has a constant offset) as the canonical
  reference point.

- **[I-3] Document the callee-saved register clobber.** `prog_main` in
  `tests/nanokernel/asm/landing_loop.asm` loads `ring_buf` into `r12` (line 68)
  and never restores it, violating the SysV callee-saved convention the crt0
  header calls "SysV-ish." It is harmless today because `crt0.asm:22–25` does no
  work after `call prog_main` (it HLT-parks). Add a comment at `prog_main`
  stating it may clobber every GPR including callee-saved `r10`/`r11`/`r12`, and
  that this is safe only because crt0 never observes registers post-call.

### Suggestions

- **[S-1]** Add a one-line in-asm rationale for the `"0" → DEFAULT_ITERS` floor
  (`tests/nanokernel/asm/landing_loop.asm:60–61`) so a future reader doesn't
  "fix" it into honoring `0` and enabling instant-exit, which would hide harness
  bugs.
- **[S-2]** Optional: cap/saturate the `.parse` accumulator past ~10 digits to
  avoid a silent `imul rax,rax,10` wrap on absurd cmdlines (line 51). Deterministic
  today, so purely defensive.
- **[S-3]** Optional: add a NASM assemble-time guard on the loop-body byte/line
  span so an accidental extra instruction breaks the build rather than surfacing
  as an icount mismatch in a hardware-gated run.
- **[S-4]** Future-proofing: expose a `guests() -> &[(&str, &[u8])]` slice from
  `lib.rs` and iterate it in `every_guest_is_a_static_x86_64_exec_at_the_load_addr`
  (`tests/nanokernel/tests/elf_shape.rs:56`) so new guests can't be forgotten in
  the shape test.
