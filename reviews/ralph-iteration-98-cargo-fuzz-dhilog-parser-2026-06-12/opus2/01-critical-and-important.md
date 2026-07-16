# Critical & Important Findings

**None.**

I found no Critical or Important issues. The change is correct and safe to merge. The points below record *why* each thing I probed is actually fine, so the next reviewer doesn't have to re-derive it.

## Verified non-issues

### Accessor coverage is total over validated bytes (reader.rs)
The fuzz target calls `body()`, `canonical()`, `aux()`, and `end()` only on a `LogReader` that `parse` already accepted. Every slice index inside `Record::body` (`reader.rs:157–205`) is dominated by a layout check in `validate_kind` (`reader.rs:516–549`) that ran during `parse`:
- `DevEvent`'s `&p[8..]` is guarded by `payload.len() >= 8`.
- `EpochHash`/`End`'s `p[8..40]` is guarded by `payload.len() == 40`.
- Fixed-size kinds (`Entropy`/`SdkEvent`/`NetTx` == 16, `TimerFire` == 20, `PadSet` == 12, `FrameMark` == 8) all check exact length.
- `NetRx`'s `frame: p` is bounded by `payload.len() <= MAX_NET_RX_FRAME`.
So `body()`'s `.try_into().unwrap()` calls cannot panic on parsed records — its infallibility is true by construction, exactly as the module doc claims.

### `end().last().unwrap()` cannot panic (reader.rs:310–320)
`parse` requires `saw_end` (`reader.rs:495–497`), so a parsed log always has ≥1 record and the last is `KIND_END` with a validated 40-byte layout. The `unreachable!` arm is genuinely unreachable. The fuzz target's unconditional `log.end()` is therefore safe on every accepted input.

### The silent `let Ok(log) ... else return` is the right choice
The probe asks whether the `ReadError` path is fuzz-relevant. It is not, and ignoring it is correct: `ReadError` derives only `Clone, Copy, Debug, PartialEq, Eq` (`reader.rs:28`) and carries only `Copy` scalars / `&'static str`. There is no hand-written `Display`, no allocation, no formatting on the error path — nothing that could panic that the fuzzer isn't already proving total by the fact that `parse` returned `Err` without aborting. Exercising `Debug`-format of the error would add no coverage of interest. No change needed.

### Integer-overflow checks ARE active (Cargo.toml `debug = 1`)
Verified locally with cargo-fuzz 0.13.2: the instrumented build receives `-Cdebug-assertions` from cargo-fuzz itself, and `overflow-checks` defaults to the `debug-assertions` value when unset — confirmed by a standalone repro where `255u8 + 1` panics under `-O -Cdebug-assertions`. The generated template's `debug = 1`-only profile is complete; adding `overflow-checks = true` would be redundant.

### `timeout-minutes: 1500` on a hosted runner is allowed
Values above 360 on GitHub-hosted runners are not an error — the job is simply hard-killed at the 6h platform cap. The inline comment already states this correctly (`nightly-drift.yaml:82–84`).
