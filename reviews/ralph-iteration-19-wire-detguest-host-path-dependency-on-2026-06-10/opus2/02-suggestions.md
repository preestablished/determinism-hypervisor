# Suggestions (non-blocking)

## S-1 — Doc comment misdescribes the empty manifest's entries as "dead"

- **File:** `crates/dh-devices/tests/detguest_host_smoke.rs:95-97`
- **Rationale:** The comment reads:
  ```rust
  // All 64 slots are present (dead entries keep their slots); nothing
  // resolves by name in an empty manifest.
  assert_eq!(m.entries.len(), REGION_CAPACITY);
  ```
  In `fresh_channel_mem`, the manifest area is zeroed, so all 64 `RegionEntry`
  slots decode with `flags == 0`. Per `RegionEntry::is_live()`
  (`detguest-wire/src/manifest.rs:179`), the DEAD bit is bit 31; with `flags == 0`
  every one of these 64 entries is **live, not dead** — they simply have empty
  names (`name_bytes()` is empty), so none matches `"telemetry"`. The "dead entries
  keep their slots" phrasing borrows the sibling crate's `RegionManifest::entries`
  doc verbatim but does not describe *this* fixture. The assertion is correct; only
  the comment's reasoning is off, which could mislead a future reader debugging a
  resolve mismatch.
- **Suggested change:**
  ```rust
  // All 64 entry slots are always returned (the manifest is fixed-capacity);
  // in an empty manifest they decode as live-but-nameless, so nothing
  // resolves by name.
  assert_eq!(m.entries.len(), REGION_CAPACITY);
  assert!(m.resolve("telemetry").is_none());
  ```

## S-2 — Duplicated `fresh_channel_mem` / manifest-build fixture vs. the sibling crate

- **File:** `crates/dh-devices/tests/detguest_host_smoke.rs:23-43`, `91-119`
- **Rationale:** `fresh_channel_mem` here re-implements the sibling crate's own
  test helper (`detguest-host/src/channel.rs:314` and `manifest.rs:176`
  `manifest_area`). Notably the sibling helpers build the manifest via the public
  `init_manifest` / `writer_begin` / `writer_end` seqlock API, whereas this test
  hand-writes a 32-byte `ManifestHeader` and pokes `generation` directly. The
  hand-rolled approach works (verified), but the seqlock-livelock case at lines
  102-118 manually sets `generation: 1` rather than using `writer_begin`, which
  ties the test to the on-wire generation encoding instead of the documented writer
  protocol. The rust-integration-testing research notes "Is shared fixture code
  deduplicated rather than copy-pasted across tests?" and warns against
  "Over-asserting on incidental details (exact byte layouts) that make refactors
  needlessly break tests."
- **Suggested change (optional):** For the livelock case, consider driving the odd
  generation through the public seqlock helper for symmetry with how the agent
  actually leaves the word odd:
  ```rust
  use detguest_wire::manifest::{writer_begin, MANIFEST_TOTAL_SIZE};
  let mut area = vec![0u8; MANIFEST_TOTAL_SIZE];
  // copy the manifest area out, writer_begin(&mut area), write it back,
  // then assert read_manifest() == Err(WireError::SeqlockLivelock)
  ```
  This mirrors the sibling's `seqlock_odd_generation_then_recovery` test and
  documents intent ("a writer began and never finished") rather than "I set the
  word to 1." Low priority — the current version is correct and self-contained.
- **Research reference:** `rust-integration-testing.md` — shared fixtures; avoid
  asserting incidental byte layout.

## S-3 — `read_manifest` happy-path could also assert recovery, not just livelock

- **File:** `crates/dh-devices/tests/detguest_host_smoke.rs:91-119`
- **Rationale:** The test asserts that a stuck-odd generation yields
  `SeqlockLivelock` (the failure bound — good, this is the subtle case). It does not
  assert the *recovery* leg (writer finishes → even generation → read succeeds).
  The sibling crate's own test does assert recovery
  (`manifest.rs:276-278`), so coverage exists upstream; adding it here would make
  the smoke test self-contained proof that the retry loop both bounds *and*
  succeeds. Optional.
- **Suggested change (optional):** After asserting `SeqlockLivelock`, bump the
  generation back to even (e.g. via `writer_end` per S-2) and assert
  `read_manifest()` is `Ok`.

## S-4 — Consider a brief note on lock-file hygiene for the promotion bead

- **File:** `Cargo.toml:23-28` (comment block)
- **Rationale:** The comment already explains why both crates are needed and points
  at bead nln. When nln promotes `detguest-host`/`detguest-wire` from
  `[dev-dependencies]` to `[dependencies]`, the `Cargo.lock` will not change (the
  closure is identical), but the production dependency graph will. A one-line
  reminder in the bead (not the code) that the promotion is a manifest-only move
  with no new lock churn would help the next agent verify the diff cleanly. No code
  change.
