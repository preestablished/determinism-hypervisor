# Action Items

Verdict: **APPROVE**. Nothing here blocks merge. The Critical/Important section is
intentionally empty; all items below are tracking notes and optional improvements.

## Action Items

### Critical

- [ ] None.

### Important

- [ ] None.

### Suggestions

- [ ] **Document the unwired guard at the source.** Add an "INTEGRATION (not yet
      wired)" line to `SlotVm::freeze_ram` (`crates/dh-vmm/src/kvm.rs`) and
      `SlotState::ensure_write_path` (`crates/dh-vmm/src/lib.rs`) naming the consuming
      beads (9e4 CoW fork → calls `freeze_ram`; ol1 slot manager → enforces
      `ensure_write_path` at write-path RPC edges; qmp → pause boundary). Verified: the
      new functions currently have **zero callers** outside their own tests, so the
      guard is dormant — make that discoverable from the code, not just the bead graph.
      (See `02-suggestions.md` S-1.)

- [ ] **File a follow-up bead for the `Faulted` slot state.** `lib.rs SlotState` has no
      `Faulted` variant though the proto carries `FAULTED_S` (API §2.8) and
      `StopReason::FAULTED` exists (API §2.4). Correctly out of scope for d2p (the R9
      fork guard), but when added it will need new transitions:
      `{Running,Paused} → Faulted` (fault at boundary), `Faulted → Empty` (DestroyVm),
      `Faulted → Paused` (RestoreSnapshot-into). File so it is tracked, not
      rediscovered. (See `01-critical-and-important.md` C-A.)

- [ ] **Enforce the `Paused`-only introspection rule at the RPC layer.** API §2.6 (line
      52) requires `ReadGuestMemory` to run only on a `Paused` slot — a `Frozen` parent
      is not a legal introspection target. This change correctly models only the *write*
      gate; the read-path admission check belongs to the future introspection RPC handler
      (not d2p). Ensure that bead checks `state == Paused` at the edge. (See C-B.)

- [ ] **Optional: pin the GPA-0 RAM-region invariant.** `ram_memfd()` resolves the memfd
      via `find_region(GuestAddress(0))`, which assumes the single RAM region lives at
      GPA 0 (true today). Add a one-line comment, or store the memfd handle on `SlotVm`
      directly to decouple "the RAM memfd" from "the region containing GPA 0". (S-2.)

- [ ] **Optional: add an `EBUSY`-on-`F_SEAL_WRITE` negative assertion to the live test.**
      The spec's load-bearing claim (`F_SEAL_WRITE` is unavailable while the KVM mapping
      lives) is documented but not exercised. Attempting `F_ADD_SEALS(F_SEAL_WRITE)` and
      asserting `EBUSY` would turn the claim into an executable fact and guard the threat
      model against environment drift. (S-4.)

- [ ] **Optional: helper for the seal bitmask.** If bead aup (preflight) branches on
      `ram_seals()`, add a small `is_frozen_sealed()` predicate so the FUTURE_WRITE
      bit-test lives in one place. (S-3.)

## Verification performed (this review)

- [x] `cargo test -p dh-vmm --lib` → **80 passed; 0 failed** (live KVM tests ran; `/dev/kvm` present and usable on this box).
- [x] `cargo clippy -p dh-vmm --lib` → clean (no warnings/errors).
- [x] Transition relation cross-checked against API §2.2 / §2.8 and ARCH §8.4 (read directly).
- [x] Confirmed introspection requires `Paused` (API §2.6 line 52).
- [x] Confirmed no external callers of `freeze_ram`/`ram_seals`/`ensure_write_path`/`can_transition`/`transition` (grep across `crates/`).
- [x] Confirmed memfd is created with `MFD_ALLOW_SEALING` (`kvm.rs:450`), so sealing is available.
