# Action Items

### Critical

_None._

### Important

- [ ] [restore_engine.rs:7-9, :84] Correct the module doc: it cites
  `DetChannelHost`'s EVTC re-attach as the load-bearing reason for RAM-first
  ordering, but `DetChannelHost` does not implement `DetDevice` (it is generic
  over `M`/`P` and its `restore` takes an extra `plan: P` arg — `detchannel.rs:96,219`),
  so it cannot be on `MmioBus` and this engine cannot drive it. Reword the
  justification as future-proofing and stop implying EVTC is on-bus today. If
  EVTC is *intended* to be restorable this iteration, file a follow-up: it needs
  a `DetDevice` impl and a plan-supplying restore seam (no producer exists for
  `tag_for_device_id(0x0001) => EVTC`).

- [ ] [restore_engine.rs:88-105, :277-284] Tighten the precondition or harden
  the engine for slot reuse. Today only `SlotState::Paused` is checked, but a
  `Paused` slot may be a previously-Running one with stale KVM dirty-ring / log
  state that this engine does not drain or reset. Either document "slot must be
  FRESH (created, never run)" or accept the `&mut DirtyRing` and drain/assert it
  empty before clearing the `DirtyPageSet`. Add a test that restores into a slot
  that executed guest code, then takes an *incremental* snapshot, to lock it.

### Suggestions

- [ ] [restore_engine.rs:215-255] Reject a bus with 0 *or* >1 pv-entropy
  (`0x0004`) devices explicitly (`entropy_device_count == 1`), instead of only
  catching the zero case; the `total_sections` check does not catch a duplicate
  entropy device. Mirror the guard on the capture side (`snapshot_engine.rs:249-251`).

- [ ] [lib.rs:64-66] Add a one-line note to the `as_any_mut` trait method:
  override ONLY when the restore engine must reach the concrete type, and the
  override must agree with the `device_id`-keyed match in the engine. No
  correctness change — prevents a future author over-overriding.

- [ ] [tests/restore_engine.rs] Add negative tests: (a) a v1 ENTR section →
  `Codec("ENTR (engine requires v2)")`; (b) a malformed-but-present device
  section (wrong length) → `device {id} rejected its section`; (c) a truncated
  VCPU section → `Codec("VCPU: ...")`. Current suite covers missing/extra
  sections but not wrong-version/wrong-length-present.

- [ ] [restore_engine.rs:287, :362] Document or rename `RestoreOutcome.pages_loaded`
  — it is always the full page count (pages materialized), not pages received
  over the wire, unlike `take_snapshot`'s `pages_shipped`.

- [ ] [restore_engine.rs:249, :324] Extract the literal `5` into a named
  `const FIXED_ENGINE_SECTIONS: usize = 5;` listing MCFG/VCPU/LAPC/TIME/ENTR,
  so a future fixed-section change flags both the check and the capture layout.
