# Action Items

Verdict: **APPROVE**. None of these block the merge; I1 should land before the engine is
wired into the production hot path, and the test/doc items should be filed as beads.

## Action Items

- [ ] **(Important, I1) Remove the redundant standalone `put_pages` call.**
  `crates/dh-worker/src/snapshot_engine.rs:223-225`. The page set is uploaded twice:
  step-3's explicit `store.put_pages(pages.clone())` and again inside
  `put_snapshot_from_parts` (`snapstore-client/src/client.rs:757`). Delete step 3, move the
  cross-check comment to the `put_snapshot_from_parts` call, and pass `pages` by move (drop
  the `pages.clone()`). Compute `pages_shipped = pages.len() as u64` before the move.
  Re-run `cargo test -p dh-worker --test snapshot_engine` (4 tests must still pass) — the
  ref values are unchanged because dedup makes the two-upload vs one-upload paths produce
  identical containers.

- [ ] **(Test gap, S2) Add a byte-determinism test.** In
  `crates/dh-worker/tests/snapshot_engine.rs`: take two FULL snapshots of identical state
  (same slot RAM, same bus, same entropy, same boundary) and
  `assert_eq!(a.snapshot_ref, b.snapshot_ref)`. Add a companion that registers the same
  device *set* on two buses in different insertion order and asserts the refs match — this
  is what makes the `KNOWN_TAGS`-position `sort_by_key` non-vacuous. Without this, the
  central determinism claim of the engine is untested.

- [ ] **(Test gap, S3) Add a multi-device-ordering test** covering EVTC (device_id 0x0001,
  KNOWN_TAGS idx 7) and BLKO (0x0005, idx 8) so the canonical sort demonstrably reorders
  relative to base-address order. In the current `test_bus()` the base order already matches
  canonical order, so the assertion cannot distinguish a correct sort from no sort.

- [ ] **(Doc/restore note, LAPC) File or annotate the restore-side bead (9wa) for the
  empty-LAPC-v1 placeholder.** The engine emits `tag::LAPC` at sec_version 1 with empty
  contents (`snapshot_engine.rs:289-292`). This is acceptable on the capture side (no
  in-kernel irqchip; injection state lives in run-control), and the FULL test asserts it's
  present-and-empty. But restore (bead 9wa) MUST be told that an empty v1 LAPC section is the
  *expected* shape, not a corrupt/truncated section, and that a future sec_version bump will
  carry the lapic-stub struct. Add a tracked note on 9wa so restore doesn't reject or
  mis-handle the empty section. (Lower-priority sub-item, S1: consider replacing the
  `agenda_empty: bool` attestation with an unforgeable witness / agenda handle when
  run-control wiring lands — file as a follow-up under the d2p state-machine epic.)

### Verification performed during this review

- `cargo test -p dh-worker --test snapshot_engine` → **4 passed, 0 failed** (live /dev/kvm +
  real in-process snapstore-server on this box).
- `cargo clippy -p dh-worker --tests` → **clean, no warnings**.
- Traced `put_snapshot_from_parts` / `build_snapshot_container` / `new_full` / `new_delta` /
  `put_pages` in the sibling `snapshot-store` checkout to confirm the hashing-delegation,
  DELTA-`guest_ram_bytes`, page-size-validation, and idempotent-retry contracts.
