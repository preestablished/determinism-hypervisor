# Positive Notes

### P1 — ACQ_REL harvest protocol matches QEMU's reference byte-for-byte

`dirty.rs:109` does `flags_atomic.load(Ordering::Acquire) & DIRTY_GFN_F_DIRTY` and `dirty.rs:122`
does `flags_atomic.store(DIRTY_GFN_F_RESET, Ordering::Release)`. This is exactly QEMU's
`accel/kvm/kvm-all.c`: `dirty_gfn_is_dirtied` = `qatomic_load_acquire(&gfn->flags) &
KVM_DIRTY_GFN_F_DIRTY`, and `dirty_gfn_set_collected` = `qatomic_store_release(&gfn->flags,
KVM_DIRTY_GFN_F_RESET)`. Critically, the store **replaces** flags with `RESET` (value `0b10`,
clearing DIRTY) rather than OR-ing `RESET` in — this is the correct kernel state transition
(`01` → `1X`), and the implementer got the non-obvious "store RESET, don't OR" detail right.

### P2 — Acquire/release placement is correct, and the slot/offset reads are ordered after the acquire

`dirty.rs:98-113`: the `slot`/`offset` raw pointers are *computed* before the acquire load but
only *read* (`slot.read()`, `offset.read()` at line 113) after the `Acquire` load establishes
the happens-before with KVM's release store of DIRTY. The comment at lines 102 explicitly
documents this ordering intent. This is the subtle correctness point of the whole protocol and
it is handled precisely.

### P3 — `kvm_dirty_gfn` field-offset assumption is sound

The code casts `addr_of!((*entry).flags)` to `*const AtomicU32`. The kernel struct is
`{ __u32 flags; __u32 slot; __u64 offset; }` — `flags` is at offset 0, naturally aligned for a
4-byte atomic, and `kvm_dirty_gfn` is `repr(C)` via kvm-bindings. The atomic cast aliases only
the `flags` word (KVM writes flags atomically; slot/offset are written before the release), so
there is no torn-read or aliasing hazard. Correct.

### P4 — mmap geometry is exactly right

`dirty.rs:61-72`: offset = `KVM_DIRTY_LOG_PAGE_OFFSET(64) * 4096 = 0x40000` as `off_t`, length =
`DIRTY_RING_ENTRIES(65536) * size_of::<kvm_dirty_gfn>(16) = 1 MiB`, `PROT_READ|PROT_WRITE`,
`MAP_SHARED` on the vCPU fd. Matches `KVM_GET_VCPU_MMAP_SIZE` ABI ("a number of pages at
KVM_DIRTY_LOG_PAGE_OFFSET * PAGE_SIZE"). `MAP_FAILED` is checked, and `Drop` (`dirty.rs:130-138`)
munmaps exactly `(ring, map_len)` — symmetric, no leak.

### P5 — Loss-free claim is accurate against the kernel's soft-full behavior

The module header's claim (`dirty.rs:15-18`) — "KVM cannot overwrite an un-RESET entry; it
exits ring-full instead" — holds. KVM uses a *soft limit*: it exits with
`KVM_EXIT_DIRTY_RING_FULL` before the ring is physically full, and if userspace keeps running
without resetting, the vCPU thread blocks on a per-VM waitqueue that `KVM_RESET_DIRTY_RINGS`
wakes. Either way the kernel never clobbers a DIRTY-but-un-RESET slot. The
`harvest_at_boundary` service path on `ExitEvent::DirtyRingFull` (`dirty.rs:188-200`,
`kvm.rs:438-442`) is the correct response.

### P6 — `slot != 0` guard correctly rejects both nonzero as_id and nonzero slot_id

`dirty.rs:116-120`: the kernel packs `slot = (as_id << 16) | slot_id`. A single `slot != 0`
check rejects any entry from a non-(as_id=0, slot_id=0) origin — exactly right for the
single-memslot v1, and it's a hard error rather than a silent skip, matching the project's
"loud divergence" philosophy. The error message includes the gfn for debuggability.

### P7 — `DirtyPageSet` bitmap math and deterministic iteration

`new` uses `mem_bytes.div_ceil(PAGE_SIZE)` then `pages.div_ceil(64)` words (verified by the
`page_set_handles_non_page_multiple_ram` test: `4096*3+1` → 4 pages). `insert` returns
newly-set and maintains `set_count` so `len()` is O(1); `iter()` walks words ascending and bits
0..64 ascending, giving the deterministic manifest order §8.5 depends on. Out-of-range gfns are
a loud `Err` (`dirty.rs:230-236`), consistent with "one slot covers [0, mem_bytes), KVM cannot
legitimately report past the end." The dense-vs-Roaring divergence is documented with the
sizing rationale (≤786k pages ⇒ ≤96 KiB) at `dirty.rs:20-24`.

### P8 — Live test quality: real end-to-end coverage with tolerant assertions

`guest_writes_are_harvested_and_ring_resets` (`dirty.rs:328-418`) runs a real-mode guest that
writes pages 0x2/0x5/0x9, harvests, asserts `>= 3` (tolerant — KVM may dirty emulation-state
pages too) with the three specific pages *present*, asserts `harvested == reset`, then does a
full **cycle 2** (rewrite rip to 0x100, dirty 0x7, re-harvest on the re-armed ring) proving the
free-running cursor advances past RESET entries, and finally `clear()` for snapshot-boundary
semantics. The cycle-2 rip rewrite after HLT is safe: HLT is a clean vCPU exit (no pending IO
to complete), so re-running after `set_regs` is a supported KVM state transition with no
hazard. `fresh_ring_harvests_nothing` covers the empty-ring base case. Both gate on
`kvm_usable()` so non-KVM CI skips cleanly.
