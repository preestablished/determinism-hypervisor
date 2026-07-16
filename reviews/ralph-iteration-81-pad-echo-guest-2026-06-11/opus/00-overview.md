# Review: pad-echo guest (bead 29a)

- **Branch:** `ralph/iteration-81-pad-echo-guest`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Base:** `main`
- **Scope:** 4 files, +114 / 157 diff lines, 1 commit

## Summary

This change adds the `pad_echo` nanokernel guest — the deterministic record/replay
exerciser for the M5 acceptance (`a5e`). Each fake frame the guest:

1. increments `F` and MMIO-writes it to pv-pad `FRAME_COUNTER` (`0xD000_101C`), which
   the device turns into the AUX `FRAME_MARK` log record (the frame-boundary exit),
2. MMIO-reads `PAD0` (`0xD000_1008`) — the only pad-input latch,
3. appends `(frame u32 LE, pad0 u32 LE)` to a RAM table at `0x30_0000` (u64 count
   header), and
4. echoes `pad0`'s low byte via `out 0x3F8, al` to the debug serial,

then runs a FIXED `PACE_ITERS=64` × 6-instruction busy loop so frame boundaries land
at deterministic icounts. Polling only — no IDT/GDT/STI (pv-pad `IRQ_VECTOR` default 0).

The plumbing additions (build.rs `PROGRAMS`, lib.rs accessor + three pinned consts,
elf_shape shape-check + a new asm↔Rust drift test) all match the established
`timer_guest` / `rep_loop` patterns.

## Verification performed

- Read the diff and all four changed files in full, plus context: `crt0.asm` (prog_main
  contract), `timer_guest.asm`, `hello.asm`, `pad.rs`, `serial.rs`, `kvm.rs` PIO
  classification, `runctl.rs` serial drain, `boot.rs` page-table/load placement, and
  ARCH §6.4 / IMPLEMENTATION-PLAN M5.
- `cargo test -p nanokernel --test elf_shape` — **7 passed, 0 failed**. `pad_echo.elf`
  assembles cleanly under NASM 2.16.01 and the new `pad_echo_asm_matches_rust_constants`
  drift test passes.

## Correctness findings (high-value)

- **Assembly is correct.** Register discipline holds across MMIO exits and across pace
  iterations. `r8`/`r9`/`r10` (PAD_BASE, TABLE_GPA, F) are preserved through the whole
  frame body — the table-append scratch (`rcx`/`rdx`) and the pace loop's
  (`rax`/`rbx`/`r11`/`r12`) never touch them.
- **The serial echo carries the right byte.** `eax` holds `pad0` from the latch read and
  nothing between the read and `out dx, al` writes `eax`/`al` (the table append only
  *reads* `eax`; `mov dx, SERIAL_PORT` writes only the low 16 bits of `rdx`). `al` ==
  pad0 low byte at the `out`. Confirmed against the documented entry layout.
- **The echo path is deterministic and logged.** `out 0x3F8, al` PIO is classified by
  `kvm.rs` as `ExitEvent::SerialOut` (PIO `0x3F8..0x400`), routed to `DebugSerial`, and
  drained to the slot log. pad_echo correctly SKIPS hello's LSR-THRE wait — sound,
  because `DebugSerial` is output-only and always reads ready (no spin hazard).
- **The FRAME_COUNTER contract is satisfied.** A fresh pv-pad starts `frame_counter=0`;
  the guest writes `F=1,2,3,…` strictly increasing, each write logging a FRAME_MARK —
  monotone, exactly as §6.4/§6.6 expect.

## Issues

No correctness defects. Two items worth raising before/at the M5 accept (both
non-blocking): an **unbounded** table with no overflow cap or documented capacity, and a
**scope note** about polling only `PAD0` (one latch) and never enabling `IRQ_VECTOR`.

## Verdict

**APPROVE**

The guest is correct, deterministic, assembles, and all tests pass. The flagged items
are forward-looking robustness/documentation notes for the `a5e` accept, not blockers
for landing this prep guest.

## Stats

| Metric | Value |
|---|---|
| Files changed | 4 |
| Lines added | +114 |
| Commits | 1 |
| Critical findings | 0 |
| Important findings | 0 |
| Suggestions | 4 |
| Tests | 7/7 pass (elf_shape) |
