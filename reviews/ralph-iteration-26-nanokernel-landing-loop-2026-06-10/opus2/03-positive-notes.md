# Positive Notes

## P1 — The loop body is genuinely 8 retired instructions, no hidden surprises

Each source line maps to exactly one instruction, the store/index/counter/branch ordering is
clean, and the terminating `jnz` retires on every iteration (taken and not-taken alike). The
`8·N` retirement identity an icount harness depends on holds exactly. For a change whose
*entire reason to exist* is icount precision, getting this dead-on matters most, and it's
correct.

## P2 — Defensive iteration-count parsing

`"0"` and empty cmdlines both fall through to `DEFAULT_ITERS` via the `test rax,rax / jz`
guard, so the guest can never be tricked into an instant exit (0 iterations) that would make
the landing test trivially pass for the wrong reason. The any-digit flag (`r10d`) cleanly
distinguishes "no digits parsed" from "parsed zero." Thoughtful.

## P3 — LCG choice makes memory state meaningful at any pause boundary

Knuth/MMIX multiplier + odd increment + `rol 13` gives a full-period, well-mixed stream;
storing it through the ring means a state hash taken at *any* instruction boundary reflects
real evolving state, not a constant or a short cycle. That's exactly what a
snapshot/replay determinism harness wants to diff.

## P4 — Correct absolute addressing for the non-PIE guest

`lea r12, [ring_buf]` and crt0's `lea rsp, [stack_top]` both encode as absolute disp32 (SIB),
which is the right call for an ET_EXEC guest pinned at its link address. No accidental
RIP-relative, no relocation needed, no `default rel` foot-gun.

## P5 — Clean test generalization without losing coverage

Refactoring `pipeline_smoke_is_a_static…` into `assert_guest_shape(name, elf)` and applying it
to both guests adds the new guest to the shape contract for free, and every assertion now
carries the guest name in its message — failures will name which guest broke. The
`default_iters_hit_the_100m_budget` test pins the headline 100M budget so a typo in either
constant is caught. The BootInfo ABI drift test remains intact.

## P6 — Honest, well-sourced comments

The asm header explains the 8-instruction contract and ties it to the Rust constant; `link.ld`
documents *why* there's no catch-all orphan sink (lld synthetic-section type mismatch) and why
a single RWE PT_LOAD is acceptable for snapshot/replay guests. These are the kind of comments
that save the next person an hour. The 64 KiB / ring-buffer / serial-`'L'` design is coherent
and minimal.
