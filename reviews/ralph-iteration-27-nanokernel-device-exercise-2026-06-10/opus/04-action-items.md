# Action Items

### Critical

- [ ] **Fix ring W descriptor size (C1).** In
  `tests/nanokernel/asm/device_exercise.asm:177`, change
  `mov dword [rbx + 0x2C], 0x1E0000` to `0x100000` (1 MiB — a power of two,
  matching `detguest_wire::header::RING_W_SIZE`). Without this, `CHANNEL_INIT`
  returns status 2, the 'D' stage parks with lowercase `d`, and the documented
  "CEPBDX" success sequence is unreachable on real hardware. Self-contained
  repro: `Channel::attach` over the bytes the asm writes returns
  `AttachError::BadRingSize { ring: W }` because `0x1E0000.is_power_of_two()`
  is false.

- [ ] **Update the layout comments to match.** Fix the module-header note
  (`device_exercise.asm` lines ~18 and ~24) that restates ring W as
  `0x20000/0x1E0000` so the clean-room layout description matches the
  implementation (`0x20000/0x100000`), not ARCHITECTURE.md §2's
  self-contradictory layout table. Optionally file a doc-bug bead against
  ARCHITECTURE.md §2 to make the table agree with its own normative
  power-of-two rule (the wire crate's `RING_W_SIZE` doc comment already
  documents this discrepancy).

### Important

- [ ] **Add a host-runnable channel regression test (I1).** Following the
  pattern in `crates/dh-devices/tests/detguest_host_smoke.rs`, write the exact
  header + Beacon bytes the asm produces into a `MockGuestMem` at `0x400000` and
  assert: (1) `Channel::attach` returns `Ok` (fails today, proving C1), and
  (2) after publishing `ringW_prod = 24`, `drain_events` yields one
  `Beacon { beacon_id: 0xB33F }`. This permanently guards the layout against
  spec-table drift and is the highest-value follow-up.

### Suggestions

- [ ] Promote the channel offsets/sizes/record constants in the asm to named
  `%define`s tied to the `detguest_wire` constant names (S1), so the C1 fix is
  self-evidently correct and future edits are harder to mis-align.
- [ ] Extend `elf_shape.rs`'s existing "asm `%define`s match Rust constants"
  pattern to cover the channel constants and the `DEVICE_EXERCISE_*` lib
  constants (S2).
- [ ] Note that `vns_sample == 0` is an acceptable Beacon `vnanos`, or sample
  `REG_ICOUNT` for a visibly nonzero stamp (S3).
- [ ] Add a one-line comment that the detcall ports are read/write on the same
  address (INIT_GO/DOORBELL answer status on the same `dx`), unlike a typical
  command/status split (S4).
