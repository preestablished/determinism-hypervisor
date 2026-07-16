# Action items — iteration 84 (bead 28i, R8)

### Critical

None.

### Important

- [ ] **I1 — Reconcile the "512" doc trail with the 1024 kernel floor.** Update
  `.agents/docs/determinism-hypervisor/IMPLEMENTATION-PLAN.md:79` (currently
  "ring size 512") and the bead `determinism-hypervisor-28i` title/description to
  "ring size 1024", with a one-line note that 512 is below the kernel's
  64 + 512-PML reserved-entry EINVAL floor on x86, so 1024 is the smallest legal
  power-of-two ring. Record the same reason in the 28i close message. The empirical
  currently lives only in the `ring_chaos.rs` preamble. (Optional: a sentence in
  ARCHITECTURE.md §8.2.)

- [ ] **I2 — Prevent map/slot ring-size mismatch.** Add
  `DirtyRing::map_for_slot(slot: &SlotVm)` that calls
  `map_sized(&slot.vcpu, slot.dirty_ring_entries)` and route all callers
  (`crates/dh-worker/tests/snapshot_engine.rs:158`, `store_durability.rs:119`,
  `restore_engine.rs:292`, `crates/dh-vmm/src/dirty.rs:336` & `:354`, and
  `ring_chaos.rs`) through it. *Or* at minimum add a `debug_assert`/doc warning on
  `map_sized` that `entries` must equal `slot.dirty_ring_entries`, and document that
  bare `map` is valid only for default-ring slots. No live bug today (all callers use
  defaults), but the symptom of a future slip is silently-dropped dirty pages — the
  exact thing this iteration exists to rule out.

- [ ] **I3 — State the acceptance substitution explicitly.** In the bead 28i close note
  (or the test preamble), record that the discharge is delta-ref *content-equality*,
  which is ≥ the planned hash-equality because `SnapshotRef = BLAKE3(manifest body
  incl. DHSNAP/vCPU + per-page index+hash table)`; that the `assert_eq!(vcpu)` is a
  redundant failure-localizer; and that a restore-and-replay leg is intentionally out
  of scope (covered by 7c8's H1==H2) since R8 concerns page loss, not restorability.
  Optionally file a follow-up bead for a literal restore-and-compare leg — not a
  blocker.

### Suggestions

- [ ] **S1 — Pin `page_dirtier` asm constants.** Add a
  `page_dirtier_asm_matches_rust_constants` test in
  `tests/nanokernel/tests/elf_shape.rs` asserting the asm `%define START_GPA` /
  `%define PAGES` equal `nanokernel::PAGE_DIRTIER_START_GPA` /
  `PAGE_DIRTIER_PAGES`, mirroring the existing `pad_echo`/`entropy_draw`/`timer_guest`
  drift-pins. The constants are exported but currently unpinned, and the
  `pages_shipped >= PAGE_DIRTIER_PAGES` floor is a `>=`, so an asm `PAGES` bump would
  go undetected.

- [ ] **S2 — Document the fork ring asymmetry.** One-line comment at
  `crates/dh-vmm/src/kvm.rs:209` noting a tier-A fork always gets the production
  65536-entry ring regardless of a custom-ring parent (forks are never created from
  chaos slots).

- [ ] **S3 — Bound the overflow count.** Align the "overflows 3 times" prose with the
  `>= 2` assertion, and consider adding an upper bound (e.g. `<= 16`) so a regression
  that fires ring-full pathologically often is caught.

- [ ] **S4 — Unify the ring-byte computation.** Use
  `ring_entries * size_of::<kvm_dirty_gfn>() as u64` at `kvm.rs:234` instead of the
  hard-coded `* 16`, and verify the now-possibly-unused `DIRTY_RING_BYTES` const
  (`kvm.rs:21`) is still referenced or drop it.

- [ ] **S5 — Link the store-hang bead.** `bd dep add <9sb> determinism-hypervisor-0vl`
  so the 128 MiB perf acceptance can't start before the 32 MiB FULL-snapshot hang is
  fixed; and when 0vl is worked, prefer a loud client-side put timeout over the current
  `ep_poll`-forever wedge.
