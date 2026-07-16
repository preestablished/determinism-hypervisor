# Action Items

### Critical

None.

### Important

- [ ] **Tie the ring map size to the slot, not a re-typed literal (I1).** Add `DirtyRing::map(slot: &SlotVm)` that calls `Self::map_sized(&slot.vcpu, slot.dirty_ring_entries)`, and route the existing default-ring callers (`dirty.rs` live_tests, `snapshot_engine`/restore tests) plus `ring_chaos.rs` through it so the entry count is never typed twice. Today `SlotVm::dirty_ring_entries` is added but consumed by nothing — make it load-bearing. Rationale: a too-small `map_sized` masks the cursor into a sub-window and silently loses dirty pages, defeating the very R8 invariant this iteration proves. File: `crates/dh-vmm/src/dirty.rs` (`DirtyRing::map`), `crates/dh-vmm/src/kvm.rs` (`SlotVm`), call sites. If deferring, file a bead.

- [ ] **Add a `bd dep` edge so 0vl blocks 9sb (I2).** Run `bd dep add determinism-hypervisor-9sb determinism-hypervisor-0vl`. 9sb's 128MiB perf acceptance will FULL-snapshot a 128MiB guest and hit the same ep_poll hang 0vl documents at 32MiB; the impact is in 0vl's prose but not in the dependency graph, so `bd ready`/`bd graph` won't surface it.

### Suggestions

- [ ] **Kill the dead `DIRTY_RING_BYTES` const and de-magic the `* 16` (S1).** `cap.args[0] = ring_entries * 16` (kvm.rs:235) is now the only ring-byte computation and `DIRTY_RING_BYTES` (kvm.rs:21) has zero consumers. Introduce `const DIRTY_GFN_BYTES` derived from `std::mem::size_of::<kvm_dirty_gfn>()` (with a `const _: () = assert!(... == 16)` pin) and use it; delete or redefine `DIRTY_RING_BYTES`. File: `crates/dh-vmm/src/kvm.rs`.

- [ ] **Add a `page_dirtier` asm-drift test and remove the dead const (S2).** Mirror `timer_guest_table_gpa_matches` / `pad_echo_asm_matches_rust_constants` in `tests/nanokernel/tests/elf_shape.rs`: parse `%define START_GPA` / `%define PAGES` from `asm/page_dirtier.asm` and assert against `PAGE_DIRTIER_START_GPA` / `PAGE_DIRTIER_PAGES`. This makes the test's `pages_shipped >= PAGE_DIRTIER_PAGES` floor (ring_chaos.rs:163) trustworthy under asm edits and wires up `PAGE_DIRTIER_START_GPA`, which is currently dead (zero consumers — verified). Files: `tests/nanokernel/tests/elf_shape.rs`; if you instead choose to drop the unused const, edit `tests/nanokernel/src/lib.rs:138`.

- [ ] **(Minor, subsumed by I1) Document `map_sized`'s power-of-two / slot-size precondition (S3).** Only relevant if I1 is not adopted and `map_sized` remains the public size-taking entry point. File: `crates/dh-vmm/src/dirty.rs`.
