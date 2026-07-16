# Positive notes

### P1 — The cross-crate coupling is now a checked fact, exactly as the bead asked

`golden_fixture_stop_reasons_decode_to_the_intended_proto_variants`
(`crates/dh-inputlog/tests/stop_reason_mirror.rs:38-52`) does the real work the bead specified:
it takes the *frozen* golden bytes (not re-derived values), runs them through the production
`LogReader::parse` → `end()` path, and decodes the `stop_reason` byte through prost's `TryFrom`,
asserting `GoalSatisfied` / `StopUnspecified`. This is the genuine mirror assertion — both sides
were individually frozen before, nothing joined them. Verified the fixtures actually carry bytes 2
and 0 (`golden.rs:99,118`) and that prost derives `PartialEq` so the `assert_eq!` is real.

### P2 — The wire-number pins are correct and target the actual hazard

I cross-checked every pin in `proto_map.rs` against the `dh-proto` codegen and its own pin test:
`SlotState` `Empty=1/PausedS=2/Running=3/Frozen=4/FaultedS=5` and `StopReason`
`BudgetReached=1/GoalSatisfied=2/HardCap=4/Paused=5/GuestHalted=6` all match. The non-contiguous
`StopReason` numbers (skipping 3 = `NextSdkEvent` and 7 = `Faulted`, which `runctl` doesn't
produce) are handled honestly — those arms are absent and the doc explains they appear "the day the
variants do," not papered over with `_ => Unspecified`.

### P3 — The "4 of 5 casts lie" trap is a genuinely instructive pin

`proto_map.rs:73` doesn't just assert the mapping is right; it *demonstrates why a naive cast is
wrong* by computing that `(domain as i32) != wire` for 4 of 5 states, with `Paused == 2`
coinciding by accident. That coincidence is the real teaching point — it's exactly why this bug
class survives spot-checks — and encoding it as a test keeps the rationale alive after the comment
rots. Arithmetic verified: only `Paused` agrees, so 4 lies is correct.

### P4 — `reader::end()`'s `unwrap` is safe in this test, and I confirmed the invariant

The new test calls `log.end()`, which does `records().last().unwrap()` (reader.rs:312). This is not
a latent panic: `LogReader::parse` rejects unsealed input with `ReadError::NotSealed` before
returning (reader.rs:375), and `EndNotLast` guards the END-position invariant, so any value the test
can hold has an END last record. Both golden fixtures are sealed. The test never constructs an
unsealed log, so the `unwrap`/`unreachable!` paths are unreachable here.

### P5 — The aarch64 cross lane genuinely passes, and for a sound reason

The iteration claims the new `dh-proto` dev-dep doesn't break the aarch64 cross-clippy lane. I ran
the documented command for the three affected crates (`--all-targets --target
aarch64-unknown-linux-gnu`) and it's clean. The reason is correct and worth recording: dh-proto's
build script sets `PROTOC` to `protoc_bin_vendored::protoc_bin_path()` — a **host-arch** binary that
runs at build time regardless of the compile target — and tonic-build emits only Rust source. There
is no target-conditional native compilation in the proto path, so cross-checking the generated code
for aarch64 is free. No build-script hazard introduced.

### P6 — Clean dead-code hygiene without suppression attributes

`slot_state_to_proto` / `stop_reason_to_proto` have no callers yet (ol1 owns those), but because
they're `pub` in a library crate they raise no dead-code warning — confirmed by building dh-worker
with no `#[allow(dead_code)]` and no warnings. The code stays honest (no blanket allows that would
also hide *real* future dead code).
