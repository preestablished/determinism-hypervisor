# Code Review — Overview

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-28-hello-elf` vs `main`
- **Bead:** determinism-hypervisor-ehu
- **Scope:** ~88-line diff — `tests/nanokernel/asm/hello.asm` (new, 28 lines), `build.rs` PROGRAMS list, `src/lib.rs` (`hello_elf()` + `HELLO_SERIAL_OUTPUT`), `tests/elf_shape.rs` shape assertion.

## Verdict

**APPROVE.** The M0 boot-path stub is correct, minimal, and well-documented. I built the
guest, ran the full `cargo test` suite (green: 3 + 1 + 3 tests), and disassembled the
emitted `hello.elf` to verify the generated machine code against the source intent. Every
scrutiny point in the review request checks out:

- `lodsb` relies on DF=0; crt0 executes `cld` before `call prog_main` — **verified in
  `asm/crt0.asm:21`**.
- `MSG_LEN` (6) exactly matches the emitted `.rodata` bytes `48 45 4c 4c 4f 0a` =
  `"HELLO\n"` — **verified by `objdump -s -j .rodata`**.
- `HELLO_SERIAL_OUTPUT = b"HELLO\n"` compiled to the correct 6 bytes (the Python-generated
  edit landed clean Rust byte-string escaping) — **verified in the literal `src/lib.rs:64`
  and consistent with the disassembled guest.**
- `msg` in `SECTION .rodata` is placed by `link.ld`'s `*(.rodata*)` rule and lands at
  `0x100040`, inside the single RWE PT_LOAD covering `e_entry == 0x100000` — **verified
  via `readelf`/`objdump`; `elf_shape` test passes.**
- The real-mode→long-mode skip is **defensible** (see 01) — the bead title's wording
  contradicts the project's own ARCH §2.3 and IMPLEMENTATION-PLAN, both of which enter
  long mode directly via `KVM_SET_SREGS`.

## Severity calibration

This is a ~30-line acceptance stub for a hypervisor that snapshots/replays its guests.
Severity is calibrated accordingly: there are **no Critical or Important findings**. The
items in `02-suggestions.md` are optional polish, not blockers.

## What I checked

- Read the full diff, `hello.asm`, `crt0.asm`, `link.ld`, and the `lib.rs` additions.
- Read ARCHITECTURE.md §2.3 and IMPLEMENTATION-PLAN.md M0.
- `cargo test` in `tests/nanokernel` — all green.
- `objdump -d` / `objdump -s` / `readelf` on the built `hello.elf`.
