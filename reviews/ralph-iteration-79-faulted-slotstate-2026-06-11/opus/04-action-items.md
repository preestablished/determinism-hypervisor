# Action items

### Critical

None.

### Important

- [ ] **File a bead: wire a `StopReason::Faulted` producer so the new edges are
  reachable.** `crates/dh-vmm/src/runctl.rs` `StopReason` (lib `runctl.rs:46-55`)
  has no `Faulted` variant, so no run-control path can currently drive
  `Running → Faulted` or `Paused → Faulted`; the state added in iter-79 is
  unreachable until a producer exists. Scope: add `StopReason::Faulted` to the
  runctl enum, emit it from the boundary/verification path on divergence /
  boundary `DATA_LOSS` / counter revocation, and (in the slot-table bead ol1)
  map that outcome onto `SlotState::transition(.., Faulted)`. Until this lands,
  `SlotState::Faulted` is decorative. Suggested: `bd create "Wire StopReason::Faulted
  producer to drive SlotState Running/Paused -> Faulted edges" -p 1 -l impl`,
  blocked-by / sequenced with ol1.

- [ ] **File a bead: pin the `dh-vmm SlotState ↔ proto SlotState` mirror
  (parallel to sr5).** The proto enum is pinned (`dh-proto/src/lib.rs:163-168`)
  and dh-vmm's relation is pinned, but nothing couples the two — and they
  **disagree on `as i32` ordering** (dh-vmm `Running` declares at index 1; proto
  `RUNNING = 3`), so a naive cast when populating `SlotInfo.state` (API.md §2.8)
  would be silently wrong. Scope: add an explicit `dh-vmm SlotState → proto
  SlotState` mapping (match, not cast) at the point ol1 reports slot state, plus
  a cross-crate test asserting the mapping (`Faulted → FAULTED_S`,
  `Frozen → FROZEN`, etc.), mirroring what sr5 does for `stop_reason`. File now,
  before ol1 lands a cast. Suggested: `bd create "Pin dh-vmm SlotState <-> proto
  SlotState mirror mapping (parallel to sr5)" -p 2 -l testing`.

### Suggestions

- [ ] **Soften the `Frozen → Faulted` exclusion comment to scope it to
  guest-contract faults.** As written ("a frozen parent executes nothing and
  accepts no writes, so nothing can fault it") it is airtight for determinism
  faults but slightly too strong for host-side integrity failures (memfd
  truncation, late seal-verification failure, `ram_seals` read error). The
  *conclusion* is correct — those go to Destroy via the existing `Frozen → Empty`
  — so keep the edge excluded; just scope the wording, e.g.: *"no
  determinism-contract fault can originate in a frozen parent; a host-side
  integrity failure on a frozen slot is a Destroy (`Frozen → Empty`), not a
  Faulted transition."* (`lib.rs:101-103`). A future `Frozen → Faulted` edge, if
  ever wanted, is one `matches!` arm + one test tuple — no stored tables to
  migrate — so deferring costs nothing.

- [ ] **(Optional) add a `fn fault(self)` helper** that only accepts
  `Running`/`Paused`, to make legal fault entry points self-documenting at the
  call site once the producer (I1) lands. Cosmetic; `transition` already fails
  closed.

- [ ] **Refresh the stale `StopReason` "mirrors proto StopReason" comment**
  (`runctl.rs:46`) when the producer lands — it currently omits `NextSdkEvent`
  and `Faulted`; state which proto variants are deferred and why.
