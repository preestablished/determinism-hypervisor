# Review — ralph/iteration-28-hello-elf (bead ehu)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-28-hello-elf` vs `main`
- **Scope:** `asm/hello.asm` M0 serial stub + nanokernel pipeline wiring (~41-line diff across 4 files)

## Verdict: APPROVE

The stub is correct, minimal, builds clean, and is fully covered by the existing
shape test. I built it (`cargo test -p nanokernel` — 7 tests green) and
disassembled the emitted `hello.elf`; every load-bearing claim in the diff holds
up against the machine code and the section layout. No Critical or Important
findings. A small set of optional suggestions follow.

## What the change is

`hello.asm` is a 28-line `BITS 64` program that does the M0 acceptance job:
`lea` the message, set count to `MSG_LEN`, `lodsb`/`out`/`loop` six bytes to the
debug-serial port `0x3F8`, then `ret` into crt0's HLT park. The Rust side adds
`hello_elf()` (an `include_bytes!` of the built ELF), the `HELLO_SERIAL_OUTPUT`
expected-bytes const, the `"hello"` entry in `build.rs`'s `PROGRAMS`, and one
line each in the smoke test and the `elf_shape` sweep.

## The central judgment call — and why the stub is right

The bead title and the M0 plan say "real-mode→long-mode stub," but this program
has **no real-mode phase**. That is correct, not a gap:

- IMPLEMENTATION-PLAN M0 (line 18): *"`dh-vmm` boots a 20-line
  real-mode→long-mode stub that writes to debug-serial and HLTs."*
- ARCHITECTURE §2.3 (lines 152–153, normative boot protocol): the freestanding-ELF
  path *"enters 64-bit mode directly (CR0/CR4/EFER/GDT set via `KVM_SET_SREGS`),
  `RIP = e_entry`, `RSI = &BootInfo`."*

So long-mode entry is the **VMM's** responsibility (the dh-cli boot bead 1mz),
performed before the guest's first instruction retires. A freestanding ELF
guest entered at `e_entry` is *already* in long mode; there is no real-mode code
for it to write. The plan's "real-mode→long-mode" phrasing is the stale artifact
(written before §2.3's direct-entry mechanism was settled), and the §2.3 normative
text wins. The author saw this exact tension and documented it in the file header
(lines 11–13), which is the right call — the discrepancy is now self-explaining
for the next reader.

## Verification performed

- `cargo test -p nanokernel` → 7 tests pass (lib 3, channel_interop 1, elf_shape 3).
- `objdump -d`/`readelf` on the built `hello.elf`: entry `0x100000`, `msg` at
  `0x100040` matching the `lea`, `.rodata` = `48454c4c4f0a` = `"HELLO\n"` (6 bytes,
  ends `0x0A`), `mov $0x6,%ecx` count correct, single RWE region covers entry+msg.
- `cargo build -p nanokernel` → no warnings (the unused `HELLO_SERIAL_OUTPUT`
  const does not trip dead-code in a lib crate).
