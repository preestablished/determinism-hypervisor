# Iteration 40 — Timer Guest (IDT delivery) — Second Independent Review

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-40-timer-guest` vs `main`
- **Bead:** 583
- **Scope:** IDT-equipped timer guest (`timer_guest.asm`), `step_one_entry` chaining
  fix (`boundary.rs`), deferral budget cap (`runctl.rs` `INJECT_DEFER_BUDGET`), three new
  live tests.

## Verdict

**APPROVE / MERGE.** This is correct, well-grounded, x86-pedantically clean work. The
guest's runtime GDT/IDT construction is byte-perfect (verified by disassembly), the
chaining fix is the right mechanism for the iter-35 two-vector hazard, and the budget cap
correctly institutionalizes the 17-minute lesson. Determinism holds across 5 fresh-process
runs (identical icount/rip/hash). All findings below are Important-at-most; none block merge.

## What I verified independently (not mirroring reviewer 1)

- **Disassembled the built ELF** (`objdump -d -M intel` + `objdump -s -j .data` +
  `objdump -t`) and checked the gate-write offset math, the GDT/IDT descriptors, and the
  `idtr`/`gdtr` limit/base-patch offsets instruction-by-instruction. All correct.
- **5x cross-process determinism**: ran the two-vector scenario five times in fresh VMs;
  boundary `(icount=80000, rip=0x1000f2, hash=99c1d186…)` identical all five.
- **Located where the post-delivery trap actually fires** by instrumenting the chaining
  loop (scratch, reverted): see 01.
- **Full `dh-vmm` suite 2x** (73 pass, 73 pass), **nanokernel suite** (green),
  **`cargo clippy -p dh-vmm --all-targets`** (clean), **`cargo fmt --check`** (clean).
- Confirmed the **iter-38 timer test** (`armed_timer_fires_and_reports_live`) still passes
  with the budget change, and that the `inject.rs` unit tests pass an explicit budget so are
  insulated from the const.

## Stats

| Metric | Value |
|---|---|
| Files changed | 6 (boundary.rs, runctl.rs, timer_guest.asm [new], build.rs, lib.rs, elf_shape.rs) |
| New live tests | 3 (two-vector, timer-ISR, masked-defer) + 1 drift test |
| dh-vmm tests | 73 pass (run 2x, identical) |
| Clippy / fmt | clean / clean |
| Critical findings | 0 |
| Important findings | 2 |
| Suggestions | 5 |
| Determinism (5x) | identical (icount 80000, rip 0x1000f2) |

## Determinism headline

5/5 runs produced an identical `(icount, rip, state_hash)` tuple. The post-delivery
single-step trap fires at **rip 0x1000e6, icount 50011** — i.e. at the *first guest
instruction retired after `iretq` returns to the interrupted spin loop*, 11 retirements
into the one entry (9-instruction ISR + the resumed `imul` + the trap landing on the next
`add`). Vector 0x41 is then queued there with IF already restored to 1 by `iretq` — no
overwrite, both vectors delivered in schedule order.
