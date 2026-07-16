# Positive Notes

### P1 — The two-leg acceptance is the correct shape for R8, and the assertions are non-tautological

The test (`tiny_ring_chaos_changes_nothing_the_snapshot_can_see`) runs the *identical* `page_dirtier` guest on two slots differing ONLY in ring size and asserts the incremental snapshot refs match. This is the right invariant: the ref is a content hash of the delta manifest, so a single lost dirty page on the chaos leg would change the manifest and break `assert_eq!(small.delta.snapshot_ref, large.delta.snapshot_ref)`. It does not re-implement the harvest logic and assert it against itself (the classic tautological-chaos-test trap from the rust-integration-testing research) — it observes a downstream, content-addressed artifact. The layered asserts (ref → `pages_shipped` → `>= PAGE_DIRTIER_PAGES` floor → bit-equal vCPU) each catch a different failure class.

### P2 — Non-vacuity is pinned on both sides

`assert_eq!(large.ring_full_exits, 0)` AND `assert!(small.ring_full_exits >= 2)` together prove the stressor actually fired on the small ring and did *not* on the large one. Without the lower bound, a build where the small ring somehow never overflowed (e.g. a botched `map_sized` or a guest that wrote fewer pages) would pass vacuously. This is the discipline that separates a real chaos test from a green-checkmark theater test, and it's explicitly called out in the doc comment.

### P3 — Honest, empirically-grounded inline documentation

The module doc and the 0vl filing don't paper over the two compromises forced by reality: (a) the bead asked for ring size 512 but the x86 kernel's 64+512 PML reserved floor rejects sub-1024 rings with EINVAL, so 1024 is the true smallest legal ring on this hardware; (b) the slot is capped at 16MiB because 32MiB FULL hangs the blocking store client (0vl). Both are documented at the point of use with the *why*, not silently swallowed. The reviewer can audit the deviation from the bead without archaeology. This is exactly the kind of "explain the constraint where the magic number lives" the codebase's other drift tests model.

### P4 — Minimal, correct plumbing; the cursor mask change is exactly right

The `map`/`map_sized` and `create_slot_vm`/`create_slot_vm_with_ring` split keeps production on the default path (`create_slot_vm` delegates with `DIRTY_RING_ENTRIES`; the CoW fork path also stays default) and adds the chosen-size path only for the test. The harvest cursor change from `% DIRTY_RING_ENTRIES` to `% self.entries` (dirty.rs:111) is the one line that had to change and it's correct: `next_harvest` is a `u64` free-running counter, so the modulo against the instance's real ring size is the only sound mask. No 64-bit overflow is reachable (~3500 harvests vs 2^64), and the per-entry `flags & DIRTY == 0` acquire-load termination (dirty.rs:127) reads correctly for a partially-filled ring at any post-wrap cursor position. The `is_power_of_two()` guard correctly rejects 0 as well (`0u64.is_power_of_two()` is `false`).
