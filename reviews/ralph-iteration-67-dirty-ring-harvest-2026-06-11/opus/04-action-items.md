# Action Items

## Critical

_None._

## Important

- [ ] **Record the §8.2/§2.2 doc divergence in bead `veu` and fix the ARCHITECTURE wording
  upstream.** ARCHITECTURE.md states (line 118, and lines 661-664) that
  `KVM_MEM_LOG_DIRTY_PAGES` is set on the RAM memslot "only on the bitmap fallback path." This
  is incorrect at the kernel level: KVM publishes dirty-**ring** entries only for memslots that
  have dirty tracking enabled, so the ring path needs the flag too —
  `crates/dh-vmm/src/dirty.rs:161-183` (`enable_dirty_logging`) correctly sets it, and the code
  is authoritative. The genuinely-exclusive part (ring enabled VM-wide forbids
  `KVM_GET_DIRTY_LOG`) is correct and should stay. Add this as "Divergence #5 (iteration 67,
  bead ygt)" to bead `veu`, and reword the two doc lines to: "`KVM_MEM_LOG_DIRTY_PAGES` is set
  on the RAM memslot on both the ring and bitmap paths (the kernel publishes ring entries only
  for dirty-tracked slots); the ring and bitmap *harvest* mechanisms are mutually exclusive per
  VM." **No source-code change required.** (See 01-critical-and-important.md I1.)

## Suggestions

- [ ] Add a one-line comment at `dirty.rs:193-199` noting the `harvested > 0` reset guard is a
  pause-boundary optimization that is always satisfied on a real `KVM_EXIT_DIRTY_RING_FULL`, so
  the resume path stays loss-free. (S1)
- [ ] In the `dirty.rs` module header, make explicit the invariant that an entry marked RESET
  but not yet `KVM_RESET_DIRTY_RINGS`-processed is a safe intermediate state (next harvest skips
  it via the clear DIRTY bit; cursor advance prevents double-count). (S2)
- [ ] Ensure the future multi-vCPU bead inherits the caveat that `stats.harvested ==
  stats.reset` (`dirty.rs:204-207`, `dirty.rs:381`) is single-vCPU-only, since
  `KVM_RESET_DIRTY_RINGS` returns a VM-wide count. (S3)
- [ ] Reuse `kvm.rs`'s `host_addr()` helper in `set_ram_flags` (`dirty.rs:165-183`) instead of
  re-deriving `userspace_addr`, so the slot's base host address has one source of truth across
  registration and flags-only re-registration. (S4)
- [ ] Optionally derive `KVM_RESET_DIRTY_RINGS` from `_IO(0xAE, 0xc7)` via a const fn /
  `nix::request_code_none!` so the `0xAEC7` encoding is verified by construction rather than by
  hand-comment. The current value is correct. (S5)

---

### Verification performed during review

- [x] Ran `cargo test -p dh-vmm --lib` on this box (has `/dev/kvm`): **84 passed, 0 failed**,
  including the 4 `dirty` tests (the real-mode guest dirtying 0x2/0x5/0x9 + harvest + reset +
  cycle-2 live test passed).
- [x] Ran `cargo clippy -p dh-vmm --lib`: clean (no warnings on `dirty`).
- [x] Verified the ACQ_REL protocol (acquire-load DIRTY, store-release `flags = RESET`,
  free-running cursor) against the kernel ABI and QEMU `accel/kvm` `dirty_gfn_set_collected` /
  `dirty_gfn_is_dirtied` — exact match.
- [x] Verified the loss-free / soft-full claim against KVM's soft-limit + waitqueue behavior.
- [x] Verified mmap geometry (offset 0x40000, len 1 MiB, MAP_SHARED on vCPU fd), drop munmap
  symmetry, `0xAEC7 = _IO(0xAE,0xc7)` encoding, `slot != 0` single-slot guard, and the dense
  bitmap math.
