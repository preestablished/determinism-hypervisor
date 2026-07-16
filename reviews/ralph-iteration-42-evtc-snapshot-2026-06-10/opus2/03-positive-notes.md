# Positive notes

### P-1 — The load-bearing state (non-reconstructible producer seqs) is captured correctly

The whole reason this bead exists is the channel ring C/I producer seqs, which the host produces and never drains — guest-sdk `channel.rs` `ProducerSeqs { ring_c: next_seq_c, ring_i: next_seq_i }`. `snapshot()` reads them via `c.producer_seqs()`, `restore()` reinstates them via `ch.restore_producer_seqs(seqs)` after re-attach, and the attached-roundtrip test proves the critical invariant: the next push after restore uses `seqs_before.ring_c + 1`, never a reused seq. This is exactly right and is the part that would have caused silent ring corruption if missed.

### P-2 — Fixed-length, branch-free framing — determinism by construction

The channel section always writes `1 + 16` bytes whether attached or not (`is_some() as u8` flag, then `unwrap_or(0)` ALWAYS emitting the u32/u64). No variable-length encoding, no `HashMap`, all little-endian scalars. The section is byte-identical for equal state with zero ordering concerns — the cleanest possible determinism story, and it makes `EVTC_LEN` a true constant.

### P-3 — Restore refuses loudly instead of attaching garbage

`restore()` validates version and exact length up front, and on the attached branch it re-validates the live channel header at the recorded GPA via `Channel::attach`, mapping any failure to `RestoreError`. The `evtc_roundtrips_detached_state_and_refuses_bad_input` test covers wrong version, truncation, and an attached-flag-with-zeroed-RAM GPA — all refuse. No silent partial restore.

### P-4 — Correct division of labor: EVTC does not double-own the queued-vector state

Consistent with the iter-38 pending-queued-undelivered-vector finding, EVTC stays out of vCPU territory: a KVM-queued-undelivered vector rides in VCPU_EVENTS and is re-queued by the M4 vCPU restore, not here. The EVTC section carries only host-side detchannel latches (init halves, status, inject latch, quiesce ack, channel gpa + seqs). No double-ownership confusion between the device section and the vCPU section.

### P-5 — Manifest correctly excluded from the wire and re-read at attach

`manifest` is guest-RAM-derived, so serializing it would be redundant and could go stale. `restore()` re-reads it from the freshly re-attached channel (`ch.read_manifest().ok()`) and increments `manifest_read_failures` on failure — matching the same metric bump path used during live attach. The restore precondition ("guest RAM already restored," §8.3 order) is correctly documented.

### P-6 — Clean gates and good test ergonomics

`cargo test -p dh-devices` passes 61 + 10 tests on two consecutive runs; clippy `--all-targets` is warning-free; fmt is clean. The test harness (`SharedMem` over `Rc<RefCell<MockGuestMem>>`, `channel_page()` building a canonical header + initialized manifest, `with_ctx`) is well-factored and lets the attached roundtrip exercise the real `pio_out`/`push_command`/`pio_in` surface rather than poking fields directly — high-confidence coverage of the actual restore behavior.
