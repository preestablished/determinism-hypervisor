# Suggestions (optional, non-blocking)

All are quality/robustness nits. None gate the merge.

## S1. Add a cheap drift test tying `HELLO_SERIAL_OUTPUT` to the asm

`landing_loop` and `bootinfo.inc` both have drift guards in `elf_shape.rs`
(`landing_loop_asm_matches_rust_constants`, `bootinfo_inc_matches_rust_constants`).
The hello path has none: `MSG_LEN 6` in the asm, `"HELLO", 10` in the asm, and
`HELLO_SERIAL_OUTPUT = b"HELLO\n"` in Rust are three independent restatements of
the same six bytes with nothing asserting they agree. If someone changes the
message in one place, the build still passes and the M0 boot test silently expects
the wrong bytes.

Lowest-effort guard (no new asm export needed) — assert the const is internally
consistent and matches the literal the loader will grep:

```rust
#[test]
fn hello_serial_output_is_six_bytes_ending_newline() {
    assert_eq!(HELLO_SERIAL_OUTPUT, b"HELLO\n");
    assert_eq!(HELLO_SERIAL_OUTPUT.len(), 6); // == MSG_LEN in asm/hello.asm
}
```

Stronger (matches the established `*_matches_rust_constants` pattern): scrape
`MSG_LEN` / the `db` line out of `asm/hello.asm` and assert it equals
`HELLO_SERIAL_OUTPUT.len()` / bytes. The prompt rightly calls this "trivial" —
treat it as nice-to-have, not required, since `elf_shape` already builds the ELF
and the bytes are visible. I lean toward the short version above: it is three
lines and closes the most likely future regression (editing the message string).

## S2. `MSG_LEN` could be derived instead of hand-maintained

`msg: db "HELLO", 10` then `%define MSG_LEN 6` restates the length by hand. NASM
can compute it: `MSG_LEN equ $ - msg` placed right after the `db`. This removes
the only place in the file where a number must be kept in sync with the string by
eye. Minor; the current form is fine for six bytes and is arguably more readable.

## S3. Header comment "20-line" vs the real count

The plan calls it a "20-line stub"; this file is 28 lines (mostly the explanatory
header). The prompt's "Calibrate to a 30-line stub" confirms the size budget is
fine — no concern, just noting the plan's "20-line" is an approximation, not a cap.
No change needed.
