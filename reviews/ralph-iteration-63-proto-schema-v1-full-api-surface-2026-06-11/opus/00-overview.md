# Proto schema v1 — full API.md §2 surface (bead bcb)

- **Branch:** `ralph/iteration-63-proto-schema-v1-full-api-surface`
- **Base:** `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Diff:** `/tmp/ralph63-diff.txt`

## Summary

This iteration fills `proto/hypervisor.proto` from the iteration-60 skeleton (§2.1 core + §2.8 GetWorkerInfo) up to the full API.md §2 surface: all 17 rpcs, every message and enum across §2.1–§2.10. I performed a message-by-message, field-by-field, number-by-number audit of the proto against the normative API.md §2 text. **Every message, field name, field type, field number, oneof tag, and enum value matches the spec exactly** — including the load-bearing tricky cases (RunRequest's `until` oneof with the non-contiguous `frame_budget = 8`, ScheduledEvent's two oneofs at 1/2/3 and 4/5/6, TakeSnapshotResponse's 12 fields, Divergence's 8, MachineConfig's 10, and all four streaming rpc signatures). The fill-in is purely additive over the skeleton — no field on a pre-existing message was renumbered. The one deliberate divergence from the original spec text — `SlotState.PAUSED → PAUSED_S` to dodge protoc's C++-scoping collision with `StopReason.PAUSED` — is correct, wire-compatible (same tag = 2), well-documented in both files, mirrored into the local API.md §2.8, and tracked for upstream sync by bead veu. `ErrorDetail`'s field numbers (1/2/3) are reasonably pinned in the proto since API.md §2.9 names the fields without numbers. The dh-proto lib.rs test additions (a 17-rpc uncalled-async-fn call-shape pin plus message round-trips) are sound patterns. `cargo build -p dh-proto` and `cargo test -p dh-proto` both pass cleanly (3 tests, 0 failures).

## Verdict

**APPROVE**

The proto is a faithful, wire-correct transcription of the normative spec; the single intentional rename is the right call and is well-handled. Findings are all non-blocking suggestions.

## Stats

| Metric | Value |
|---|---|
| Files changed | 3 (`proto/hypervisor.proto`, `crates/dh-proto/src/lib.rs`, `.agents/docs/.../API.md`) |
| Lines added / removed | +560 / −16 |
| rpcs audited | 17 / 17 — all match |
| Messages audited | 50+ — all field numbers match |
| Enums audited | 6 (HashEpochs, MemPredicate.Op, StopReason, PixelFormat, SlotState, QuiesceMode) — all match |
| Deliberate spec divergences | 1 (PAUSED → PAUSED_S, documented + tracked by bead veu) |
| Critical | 0 |
| Important | 0 |
| Suggestions | 4 |
| `cargo build -p dh-proto` | PASS |
| `cargo test -p dh-proto` | PASS (3 passed) |
