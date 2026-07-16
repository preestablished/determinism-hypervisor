# Critical & Important findings

## Critical

None.

---

## Important

### I-1 — `--json` mode emits invalid JSON for non-printable serial bytes

**File:** `tools/dh-cli/src/main.rs`, `boot_cmd` JSON branch (diff lines 368–374).

```rust
let escaped: String = out
    .serial
    .iter()
    .flat_map(|b| std::ascii::escape_default(*b))
    .map(char::from)
    .collect();
println!("{{\"serial\":\"{escaped}\",\"exits\":{}}}", out.exits);
```

`std::ascii::escape_default` produces Rust/C-style escapes, **not** JSON escapes. For
any byte outside the printable-ASCII set that isn't one of `\n \r \t \\ \"`, it emits
`\xNN`. JSON has no `\xNN` escape — the only legal escapes in a JSON string are
`\" \\ \/ \b \f \n \r \t` and `\uXXXX`. So the moment a guest writes a control byte to
the serial port, the `--json` output is **malformed JSON** and any consumer (`jq`, a
test harness, a CI assertion) will fail to parse it.

Verified empirically:

| byte | `escape_default` output | valid JSON? |
|---|---|---|
| `0x07` (BEL) | `\x07` | **no** |
| `0x00` (NUL) | `\x00` | **no** |
| `0x1b` (ESC) | `\x1b` | **no** |
| `0x0a` (LF)  | `\n`   | yes |
| `0x22` (`"`) | `\"`   | yes |
| `0x5c` (`\`) | `\\`   | yes |
| `0x48` (`H`) | `H`    | yes |

`HELLO\n` happens to escape cleanly (only `\n`), which is why the live acceptance never
tripped it. But the device-exercise sequence and any future binary serial output will
break the contract. Severity is **Important rather than Critical** because:

- it is a debug-only output mode on a debug-only CLI (ARCH §1),
- the human-readable (non-`--json`) path is unaffected (it `write_all`s raw bytes),
- no current test exercises `--json` with non-printable output.

**Fix (smallest correct form):** escape per the JSON spec — emit `\uXXXX` for control
bytes (and the named short escapes where they exist), pass printable ASCII through:

```rust
fn json_escape(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len());
    for &b in bytes {
        match b {
            b'"'  => s.push_str("\\\""),
            b'\\' => s.push_str("\\\\"),
            b'\n' => s.push_str("\\n"),
            b'\r' => s.push_str("\\r"),
            b'\t' => s.push_str("\\t"),
            0x20..=0x7e => s.push(b as char),
            other => s.push_str(&format!("\\u{other:04x}")),
        }
    }
    s
}
```

Note this also makes the field unambiguously a byte-stream-as-string. If the intent is
that consumers reconstruct exact bytes, `\uXXXX` is correct for `< 0x80`; for `>= 0x80`
you are emitting a single code point per byte which round-trips only if the consumer
treats the string as Latin-1 — acceptable for a debug field but worth a one-line doc
comment. (A base64 field would be unambiguous but is heavier than this CLI warrants.)

**Recommend:** add a regression test that boots a guest emitting at least one control
byte with `--json` and pipes the output through a JSON parser, so this cannot regress
once a non-`HELLO` guest is wired in.
