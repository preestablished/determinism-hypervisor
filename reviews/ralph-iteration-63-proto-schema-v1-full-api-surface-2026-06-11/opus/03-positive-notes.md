# Positive Notes

## Field-number fidelity is flawless

Across 50+ messages, 6 enums, and 17 rpcs, **every** field name, type, and number matches the normative API.md §2 text. This is the kind of change where a single transposed digit is a silent wire break months later, and the transcription is exact — including the deliberately awkward cases:

- `RunRequest.until` keeps `frame_budget = 8` non-contiguous (it was appended after the original 2–5 arms), rather than "tidying" it to 6 (`proto/hypervisor.proto:178`). Resisting that tidy-up is exactly right: renumbering an existing wire field is the break the audit exists to catch.
- `ScheduledEvent`'s two independent oneofs (`at`: 1/2/3, `event`: 4/5/6) preserve the spec's interleaving of tag space across both oneofs (`proto/hypervisor.proto:146-165`).
- `TakeSnapshotResponse`'s 12 fields, with the capture-output trio (`feature_bytes=9, fb_lz4=10, fb_info=11`) appended *after* `determinism_class=8` and `frame_counter=12` last, all match (`proto/hypervisor.proto:251-270`).

## The additive-only discipline held

The header (`proto/hypervisor.proto:8-11`) commits to "purely additive (no renumbering)" over the v8p skeleton, and the diff confirms it: the pre-existing §2.1 core (`SnapshotRef`/`StateHash`/`Lease`) and §2.8 `GetWorkerInfo`/`DeterminismClass` blocks are untouched; everything else is new text slotted in. No existing tag moved.

## PAUSED_S is the right fix, handled the right way

Choosing to rename the *value symbol* while preserving the tag (`= 2`) is the wire-safe way to dodge protoc's C++-scoping collision, and reusing the exact `_S` convention the spec already established for `FAULTED_S` keeps the workaround self-consistent and self-documenting. Editing the local API.md §2.8 to match (rather than letting proto and spec silently diverge) plus tracking upstream sync via a bead is textbook handling of a "spec is wrong, here's the minimal correction" situation.

## The 17-rpc compile-time pin is a genuinely good pattern

`_all_seventeen_rpcs` (`crates/dh-proto/src/lib.rs:48-90`) turns "did codegen actually emit every method with the right request type?" into a *compile* error rather than a runtime gap. The doc-comment (lines 43-47) correctly diagnoses *why* the naive alternatives don't work — tonic methods take `impl IntoRequest<T>`, so you can't pin them as bare fn items; you need a real call expression — and the `all_seventeen_rpcs_are_generated` test references the fn to keep dead-code lints from deleting it. That's a well-reasoned, self-explaining pin.

## The optional-presence test asserts the right thing

`NextSdkEvent { stream: None }` vs `{ stream: Some(9) }` asserting *different* encoded bytes (`crates/dh-proto/src/lib.rs:171-173`) is precisely the assertion that distinguishes proto3 `optional` (presence-tracked `Option<u32>`) from a plain `uint32` (where `None` and `0` would be wire-indistinguishable). It pins the `optional` keyword, not just the type.

## Comments are accurate where they claim verbatim

Spot-checking the ported semantic comments against API.md: `RunRequest.frame_budget` (proto 178-185 vs API.md 205-212), `ScheduledEvent.at_frame` (proto 150-158 vs API.md 175-183), and `TakeSnapshotResponse` field 12 (proto 267-269 vs API.md 296-298) are character-for-character matches. The "keep both in sync" instruction in the header is backed by actual fidelity.

## Tests pass cleanly from a fresh proto rebuild

After `touch proto/hypervisor.proto` to force build.rs to re-run protoc, `cargo build -p dh-proto` and `cargo test -p dh-proto` both succeed — 3 tests pass, 0 failures, 0 warnings. The generated code genuinely matches the new schema; the tests are not stale against a cached build.
