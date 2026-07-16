# Iteration 81 — pad_echo guest — Review (2nd reviewer)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-11
- **Branch:** `ralph/iteration-81-pad-echo-guest`
- **Scope:** 4 files, +114 lines — new M5 record/replay guest `pad_echo.asm`, `build.rs` PROGRAMS entry, `lib.rs` accessor + 3 consts, `elf_shape.rs` shape assert + drift test.

## Summary

`pad_echo` is the M5 record/replay guest: an infinite frame loop that writes
`FRAME_COUNTER` (MMIO `0xD000_101C`), polls `PAD0` (MMIO `0xD000_1008`),
appends `(frame u32, pad0 u32)` pairs to a RAM table at `0x30_0000` behind a
u64 count header, echoes pad0's low byte to PIO `0x3F8`, and paces frames with
a fixed 64-iteration 6-instruction busy loop so every frame boundary lands at a
deterministic icount.

The assembly is **correct**. I assembled it with nasm (`-f elf64`) — clean, and
the symbol table is as expected (`prog_main`, `.frame`, `.pace`, `work_buf`).
I verified the full ASM instruction-by-instruction, the MMIO/PIO device reality
against `dh-devices` (`pad.rs`, `bus.rs`, `serial.rs`), the loader contract
(`boot.rs`), the bss/RAM zeroing story, and the drift test parser. The
torn-read discipline (entry written before count increment), the `al`-survives-
to-`out` claim, the high-half register hygiene, and the 4-byte aligned MMIO
offsets all check out.

The findings are not bugs in what this code *does* — they are about what the
guest leaves **unbounded and un-pinned** for the M5 acceptance run that will
schedule against it. The headline is a **table-vs-RAM overflow** that arrives
far sooner than a 60 s-vns run: at the test-default 64 MiB the table runs off
the end of guest RAM after ~3 s-vns of frames, long before any plausible M5
horizon. There is no cap, no mask, and no documented capacity — the guest just
walks its writes into unmapped GPA.

## Verdict

**Approve with comments.** The diff is mergeable as a guest binary + drift pin;
nothing here is wrong on its own. But the M5 run that consumes it MUST NOT be
written assuming this guest can free-run for 60 s-vns — file the capacity issue
before that schedule lands, or the acceptance run faults mid-table. The
unbounded-table behavior should at minimum be **documented with a concrete
frame/RAM budget**, and ideally capped or masked.

## Stats

| Category   | Count |
|------------|-------|
| Critical   | 0     |
| Important  | 2     |
| Suggestions| 4     |
| Positive   | 6     |

The two Important items are the unbounded table overflow (latent, fires in the
consuming M5 run) and the incomplete drift pin (REG_PAD0 / REG_FRAME /
SERIAL_PORT / entry-size not pinned against the device-side truth).
