# Review Overview — iteration 40: timer_guest (IDT-equipped interrupt guest)

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-40-timer-guest` vs `main`
- **Bead:** determinism-hypervisor-583
- **Scope:** `tests/nanokernel/asm/timer_guest.asm` (new M3 interrupt guest),
  `boundary::step_one_entry`, `runctl` entry-chaining + `INJECT_DEFER_BUDGET`,
  `nanokernel` lib exports, `elf_shape` drift test.

## Verdict

**APPROVE.** The change is x86-correct, deterministic, and the central iter-35
demand — *two vectors at one boundary must both deliver, in order, bit-identical
across boots* — is now proven by a live test. Every descriptor, gate, and limit
encoding verifies against the SDM. No Critical or Important findings. A small
number of low-risk suggestions (documentation precision on the `step_one_entry`
contract; an asm robustness nicety) are recorded but none block merge.

## What was verified live (this host, /dev/kvm rw)

| Check | Result |
|---|---|
| `cargo fmt --all --check` | clean |
| `cargo clippy --all-targets --all-features` | clean, zero warnings |
| 3 new `idt_guest_tests` x5 runs | 15/15 pass, ~2s each, zero flakes |
| `dh-vmm` full lib suite | 73 passed, 0 failed |
| `nanokernel` incl. `timer_guest_table_gpa_matches` drift test | all pass |
| GDT code64 `0x00209A...` bit decode | type=0xA, S=1, P=1, L=1, D=0 — correct 64-bit code |
| GDT data `0x000092...` bit decode | type=0x2, S=1, P=1 — correct RW data |
| Gate attr word `0x8E00` at `[+4]` | byte4=IST=0, byte5=0x8E → P=1 DPL=0 S=0 type=0xE 64-bit interrupt gate |
| IDTR limit `0x42*16-1` | `0x41F`, covers vectors 0..0x41 inclusive |
| TABLE_GPA 0x200000 vs guest extent | guest .bss tops at ~0x107440; 0x200000 is untouched, zeroed, identity-mapped 2 MiB page, present+writable |

## Stats

- Files changed: 6 (+423 / −8)
- New asm guest: 140 lines
- New engine fn: `step_one_entry` (42 lines)
- New tests: 3 live (`idt_guest_tests`) + 1 drift (`timer_guest_table_gpa_matches`)
- Findings: **0 Critical, 0 Important, 3 Suggestions**
