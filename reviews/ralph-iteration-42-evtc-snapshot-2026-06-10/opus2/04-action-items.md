# Action items

### Critical

None.

### Important

- [ ] **Resolve the reused-slot state leak in `DetChannelHost::restore()`** (`crates/dh-devices/src/detchannel.rs`). `restore()` rewrites the eight snapshotted fields but leaves `self.metrics`, `self.last_drain_error`, and `self.responder` (which owns `TableFaultPlan.hits` occurrence counters) at their pre-restore values. For a fork CHILD built via `new()` this is correct (clean slate). For an in-place restore of a slot reused across tenants (§8.3 / slot table), the prior tenant's anomaly counters AND — the actual determinism hazard — its occurrence hit counts leak into the next session, changing inject decisions. The sibling `PvBlk` device serializes its anomaly counter (`host_io_errors`), so "ignore them" is inconsistent with the DHSNAP convention either way. Choose and document one: (a) reset `metrics`/`last_drain_error`/responder-plan counters at the top of `restore()` — prioritize the responder reset, it is the part that affects replay determinism; (b) serialize `metrics`/`last_drain_error` into the layout to match blk (bump `EVTC_LEN`/`EVTC_VERSION`); or (c) document the precondition that `restore()` is only ever called on a freshly-`new()`'d host and add a debug-assert. Option (c) is the minimum bar; option (a)'s responder reset is mandatory if slots are reused in place.

- [ ] **Add a §8.4 fork breadcrumb to the `restore()` doc comment** (`crates/dh-devices/src/detchannel.rs`, ~line 206). The doc cites only §8.3. Note that this same `restore()` is the §8.4 Tier-A same-worker fork path: a fresh child host restores from the parent's in-memory EVTC bytes, and `self.mem` is the child's CoW `MAP_PRIVATE` mapping so re-attach binds to the child's view (verified correct). Doc-only; de-risks the downstream fork bead.

### Suggestions

- [ ] **S-1:** Add a `snapshot().len() == EVTC_LEN` assertion for the detached case (the attached case is already covered); the constant is hand-maintained and currently decoupled from the writer. `EVTC_LEN = 39` is correct as written.
- [ ] **S-2:** Add a `snapshot -> restore -> snapshot` byte-identical-roundtrip test mirroring `blk.rs::restore_then_snapshot_is_byte_identical_and_keeps_host_io_errors`, to lock in the canonical (scalar-only) section.
- [ ] **S-3:** Add an explicit out-of-range-GPA restore test variant (e.g. `0xDEAD_0000`) so the "attach Mem error -> refusal" branch is named, distinct from the existing zeroed-valid-GPA bad-header test (same code path, better-documented coverage).
- [ ] **S-4:** When adding the I-2 fork breadcrumb, note the child also inherits the degraded inject-name-resolution window (empty intern caches) until its replay re-seeds them, so the fork bead's author wires the cache-replay step alongside the seq restore.
