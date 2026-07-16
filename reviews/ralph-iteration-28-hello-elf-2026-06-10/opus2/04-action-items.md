# Action items

## Blocking (must fix before merge)
- None.

## Recommended (optional, low effort)
- [ ] **S1** — Add a 3-line drift test asserting `HELLO_SERIAL_OUTPUT == b"HELLO\n"`
  and `.len() == 6` (mirrors `MSG_LEN` in `asm/hello.asm`). Closes the most likely
  future regression: editing the message in one of its three restatements. See
  `02-suggestions.md`.

## Nice-to-have (take or leave)
- [ ] **S2** — Derive `MSG_LEN equ $ - msg` instead of the hand-coded `6`, so the
  count can't drift from the string.

## Follow-up owned elsewhere (not this bead)
- [ ] The actual M0 acceptance — `dh-cli boot tests/nanokernel/hello.elf` printing
  the expected bytes and exiting — lands in bead **1mz** (dh-cli boot), which is the
  consumer of `hello_elf()` and `HELLO_SERIAL_OUTPUT`. Nothing for *this* bead.

## Verification log (for the record)
- `cargo test -p nanokernel` → 7 passed, 0 failed.
- `objdump -d` / `readelf -h` / `objdump -s -j .rodata` on built `hello.elf`:
  entry `0x100000`; `msg` @ `0x100040` == the `lea`; `.rodata` = `48454c4c4f0a`
  = `"HELLO\n"` (6 bytes, ends `0x0A`); `mov $0x6,%ecx`; `cld` precedes `call`
  with no DF clobber; single RWE PT_LOAD covers entry and msg.
- `cargo build -p nanokernel` → no warnings.

**Verdict: APPROVE — merge as-is; S1 recommended as a fast follow.**
