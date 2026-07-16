# Suggestions (non-blocking)

## S-1. Document the unwired guard so it cannot silently rot

`SlotVm::freeze_ram` / `ram_seals` (`kvm.rs`) and `SlotState::ensure_write_path` /
`can_transition` / `transition` (`lib.rs`) currently have **zero callers** outside
their own definitions and tests (verified by grep across `crates/`). This is correct
for d2p — wiring is beads 9e4 (CoW fork calls `freeze_ram` on the parent), qmp
(snapshot pause boundary), and ol1 (slot manager drives the transitions). But an
unwired-but-public guard is exactly the thing that rots: a future refactor could
mutate a `Frozen` slot's RAM and nothing would call `ensure_write_path`.

The doc-comments already gesture at this ("the SOFTWARE Frozen state … is the guard"),
but they describe the *mechanism*, not the *integration debt*. Recommend one explicit
line on `freeze_ram` and on `ensure_write_path` naming the consuming beads, e.g.:

```rust
/// INTEGRATION (not yet wired): called by the CoW-fork path (bead 9e4) on the
/// parent at freeze time; the Frozen write-denial is enforced by the slot
/// manager (bead ol1) at every write-path RPC edge. Until then this guard is
/// dormant — do not assume Frozen RAM is protected at runtime yet.
```

This makes the dormancy discoverable from the source, not just from the bead graph.

## S-2. `find_region(GuestAddress(0))` assumes RAM region 0 is the memfd-backed one

`ram_memfd()` resolves the memfd via `find_region(GuestAddress(0))`. Today
`create_slot_vm` builds exactly one region at GPA 0 backed by the memfd (`kvm.rs:158-163`),
so this is correct. But it silently couples "the RAM memfd" to "whatever region
contains GPA 0". If a future layout ever puts a non-memfd region (or a hole) at GPA 0,
`ram_memfd` would mis-resolve or return the no-memfd error. Consider either a short
comment pinning the invariant ("guest RAM is a single GPA-0 region; revisit if layout
splits") or, longer-term, storing the memfd handle on `SlotVm` directly rather than
re-deriving it through the region table each call. Low priority — purely future-proofing.

## S-3. `ram_seals` returns a raw `i32`; consider naming the seal bits

`ram_seals() -> Result<i32, KvmError>` hands back the raw `F_GET_SEALS` bitmask, and
tests read it with `& libc::F_SEAL_FUTURE_WRITE`. That is fine for a probe. If bead aup
(preflight) starts branching on it, a tiny helper (`fn is_frozen_sealed(&self) -> bool`
checking the FUTURE_WRITE bit) would keep the bit-twiddling in one place and read better
at the preflight call site than an inline mask. Optional.

## S-4. Live test: assert `EBUSY` reasoning is captured, not just `EPERM`

The test proves a *new* writable mmap is `EPERM` and the existing mapping stays writable.
The spec's subtle point is *why* `F_SEAL_WRITE` was not used: it would `EBUSY` because the
parent's KVM mapping is a live writable shared mapping. The diff's comments explain this
well, but the test does not exercise it (it never attempts `F_SEAL_WRITE`). A one-line
negative assertion — attempt `F_ADD_SEALS(F_SEAL_WRITE)` and assert it fails with `EBUSY`
while the mapping lives — would turn the spec's load-bearing claim into an executable fact
and guard against a future kernel/setup change that quietly makes `F_SEAL_WRITE` succeed
(which would change the threat model). Nice-to-have, not required.
