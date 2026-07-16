# Action Items

Self-contained checklist from the 2nd-reviewer pass on
`ralph/iteration-73-snapshot-engine-takesnapshot` (bead qmp,
`crates/dh-worker/src/snapshot_engine.rs`). No Critical/Important items —
all optional. Verdict: **APPROVE**.

## Blocking (Critical / Important)

- [ ] *(none)*

## Non-blocking (Suggestions)

- [ ] **S1 — Stop uploading pages twice.**
  `snapshot_engine.rs:223-225` calls `store.put_pages(pages.clone())`, then
  `put_snapshot_from_parts` (line 232) internally calls `put_pages` *again*
  (`snapshot-store/.../snapstore-client/src/client.rs:757`). Either delete
  the explicit step-3 `put_pages` (preferred — also removes a `pages.clone()`),
  or, if the early upload is intentional for fail-fast, document that in the
  step-3 comment. Re-run `cargo test -p dh-worker --test snapshot_engine`
  after.

- [ ] **S2 — Document the empty-delta contract.** An `Incremental` snapshot
  with no dirty pages yields a valid zero-page DELTA (`parent=Some`,
  `entries=0`), confirmed end-to-end. Add a line to the
  `PageSource::Incremental` (or `take_snapshot`) doc stating this is
  intended and not an error — or add an explicit guard if it should be
  rejected. Pick one and document it.

- [ ] **S3 — Note the `agenda_empty` / `SegmentOutcome` seam for ol1.** The
  bool is the honest attestation today (no persistent agenda object exists
  after `run_segment`). When ol1 (slot table) lands, consider sourcing
  `icount/vns/hash_chain` from `runctl::SegmentOutcome`
  (`crates/dh-vmm/src/runctl.rs:59`, which already carries `boundary`,
  `vns`, `state_hash`) instead of a hand-built `BoundaryState`. Track on the
  ol1 bead, not this one.

- [ ] **S4 — (optional) Guard `mem_bytes` page-alignment.** Add
  `debug_assert!(slot.mem_bytes.is_multiple_of(PAGE_SIZE))` at the top of
  `take_snapshot`, or validate in `create_slot_vm`
  (`crates/dh-vmm/src/kvm.rs:121`), so an unaligned slot fails locally
  instead of far away at `Manifest::new_full`. Low priority — no current
  caller hits it.

## Verification performed this pass (no action needed)

- [x] Byte determinism, same slot: two `Full` snapshots → identical ref.
- [x] Cross-VM identity: two independent slots+buses, same seed/config →
  identical ref.
- [x] Empty delta is valid (0 pages, `parent=Some`, `entries=0`).
- [x] BLKO section sorts to canonical order and round-trips.
- [x] `cargo test -p dh-worker --test snapshot_engine` — 4 pass × 2 runs.
- [x] `cargo clippy -p dh-worker --tests` — clean.
- [x] Working tree clean (all scratch experiments reverted).
