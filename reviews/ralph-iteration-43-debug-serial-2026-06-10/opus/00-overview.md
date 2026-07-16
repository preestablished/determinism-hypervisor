# Review: iteration 43 — debug-serial (ARCH §6.9)

- **Branch:** `ralph/iteration-43-debug-serial`
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus
- **Commit under review:** `git diff main...HEAD` (one commit, 166d9e6)

## Summary

This iteration implements the ARCH §6.9 debug-serial model as `DebugSerial` in
`crates/dh-devices/src/serial.rs` (a 16550-subset, output-only state machine), wires it into
both dh-cli debug boot loops (`boot.rs` run-until-HLT and `run.rs` segment `on_exit`), and
upgrades `hello.asm` to poll LSR before each byte — which live-proves the IN path that the
pre-avm blanket `data.fill(0)` would have spun on forever (the iter-29 hazard). I read every
changed file plus the things they touch (the `DetDevice` trait and `MmioBus` contract in
`bus.rs`, `device_sections` framing in `hash.rs`, the `classify_exit` IN-FILL contract in
`kvm.rs`, the `land_at`/`step_one_entry` exit handling in `boundary.rs`, and the sibling device
impls `pad.rs`/`entropy.rs`/`clock.rs` for snapshot/restore convention), confirmed §6.9 + §2.2
conformance (device-id 0x0006 unique and sequential; MMIO mirror at 0xD000_6000; PIO 0x3F8+8;
register slots at `0x08 + reg*4`), and ran the suites live on the lab box: `cargo test -p
dh-devices` (65 passed), `cargo test -p dh-cli --test boot_hello` (4 passed including the live
LSR-polling boot and the run-twice determinism check), full `cargo test -p dh-cli`, and
`cargo clippy -p dh-devices` — all green. The model is pure (no host time/randomness/IO),
the empty snapshot is well-formed against the `(id, version, len, bytes)` framing, restore is
strict, and determinism is the explicit and correct bar over hardware fidelity.

## Verdict

**APPROVE**

## Stats

- Files changed: 7 (+244 / −16)
- New code: `crates/dh-devices/src/serial.rs` (199 lines, 4 unit tests)
- Tests run live: dh-devices unit (65 ok), boot_hello (4 ok), dh-cli full suite (ok), clippy (clean)
- Critical issues: 0
- Important issues: 0
- Suggestions: 4
