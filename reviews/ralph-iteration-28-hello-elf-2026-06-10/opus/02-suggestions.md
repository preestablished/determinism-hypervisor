# Suggestions (optional polish — none blocking)

All items below are nice-to-haves for a 30-line stub, not defects.

## S1 — Fix the stale "real-mode→long-mode" wording in IMPLEMENTATION-PLAN.md M0

`IMPLEMENTATION-PLAN.md:18` still reads "real-mode→long-mode stub", which contradicts
ARCHITECTURE.md §2.3 (long mode entered directly via `KVM_SET_SREGS`). The asm header already
documents the discrepancy guest-side, but the plan itself is the source the *next* agent will
read and be misled by. A one-line edit — e.g. "long-mode serial stub that writes to
debug-serial and HLTs" — removes the contradiction at the root. Out of scope for this bead;
worth a follow-up.

## S2 — Derive `MSG_LEN` instead of hardcoding it

`hello.asm` hardcodes `%define MSG_LEN 6` next to `msg: db "HELLO", 10`. nasm can compute
the length so the two can never drift:

```asm
msg:    db  "HELLO", 10
MSG_LEN equ $ - msg
```

For a constant 6-byte string this is cosmetic, but it makes the string self-describing if
anyone edits the message later. (Note: `MSG_LEN` as a `%define` vs an `equ` is also a minor
style point — `equ` is the idiomatic nasm form for an assemble-time constant.)

## S3 — Consider asserting serial output, not just ELF shape

`HELLO_SERIAL_OUTPUT = b"HELLO\n"` is currently exported but only the *embedded-and-nonempty*
and *elf-shape* invariants are tested in-tree. The actual "guest boots and emits these bytes"
assertion lives in the dh-cli boot bead (1mz), per the comments. That's a reasonable split,
but if a fast host-side smoke (e.g. a unit test that scans the emitted `.rodata` for
`HELLO_SERIAL_OUTPUT`) is cheap, it would catch a msg/const mismatch without a full boot.
Optional — the existing coverage is adequate for M0.

## S4 — `mov rcx, MSG_LEN` loads a 64-bit immediate where 32 would do

The assembler already emitted `mov $0x6,%ecx` (the 32-bit form, which zero-extends), so
nasm optimized this for you and there is nothing to change. Noting only for completeness:
`loop` uses the full RCX, and the upper bits are guaranteed zero by the `ecx` write, so the
loop count is correct. No action.
