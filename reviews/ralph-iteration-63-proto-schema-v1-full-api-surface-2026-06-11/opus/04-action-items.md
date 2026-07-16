# Action Items

Branch: `ralph/iteration-63-proto-schema-v1-full-api-surface` — Reviewer: Claude Opus — 2026-06-11

### Critical

- [ ] _None._ The field-number fidelity audit (17 rpcs, 50+ messages, 6 enums, all numbers) found zero wire-contract mismatches against API.md §2. No action required.

### Important

- [ ] _None._ The PAUSED_S rename, the 17-rpc compile pin, the message round-trips, and the proto3-`optional` handling are all correct. `cargo build -p dh-proto` and `cargo test -p dh-proto` pass (3 tests). No action required.

### Suggestions

- [ ] **S1 — Refresh the proto PAUSED_S comment.** `proto/hypervisor.proto:408` still says "API.md §2.8 wrote PAUSED here," but the local API.md §2.8 (line 441) is already patched to `PAUSED_S` on this branch, so the past-tense note describes the unsynced *upstream* copy. Reword to say the local spec is patched and reference bead veu inline. (Cosmetic; non-blocking.)
- [ ] **S2 — Reconcile the "comments transcribed exactly" header with the one intentional divergence.** The PAUSED_S note is the single place where the proto comment (lines 406-409) and the API.md comment (lines 443-445) are paraphrases rather than verbatim. Either align the wording or soften the header at `proto/hypervisor.proto:8-11` so a future verbatim audit doesn't flag it.
- [ ] **S3 — Move the Divergence fixture off the spec bound.** `crates/dh-proto/src/lib.rs:156-157` uses `icount_lo: 100, icount_hi: 1124` (`hi - lo == 1024`, exactly the API.md §2.7 `≤ 1024` limit). Harmless to the test, but consider an inside-the-bound value (e.g. `icount_hi: 1100`) so the fixture reads as a typical window. (Optional.)
- [ ] **S4 — Optionally pin the `frame_budget = 8` oneof tag directly.** The round-trip test already exercises `FrameBudget(60)`; a small extra assertion that the encoded oneof key for the `frame_budget` arm is field 8 (varint key `0x40`) would catch a future silent renumber that a self-consistent round-trip cannot. (Optional belt-and-suspenders; protoc + the proto text is the primary guard.)
