# Action Items

### Critical

_None._

### Important

- [ ] **Reconcile the API.md §4 NETL row with the regs-only reality and log it in `veu`.**
  The row at `.agents/docs/determinism-hypervisor/API.md:623` reads
  `NETL | pv-net regs + pending-RX state (must be empty at snapshot; enforced)`, but the
  landed NETL section (`crates/dh-devices/src/net.rs`, `SECTION_LEN = 36`) is registers
  only — there is no separate pending-RX state field and no runtime "enforced" check; the
  empty-pending-RX invariant holds *by construction* because the device buffers no frame.
  Amend the local row to say the section is the 7 registers (36 bytes) and that the §4
  empty-pending-RX rule holds structurally (no queue exists), then add a divergence entry
  to bead `determinism-hypervisor-veu` (the running code↔doc divergence ledger, alongside
  its existing #8/#9) so the next upstream `.agents/docs` sync does not revert the fix.

- [ ] **Resolve the zero-length `NET_RX` log/device asymmetry (file a bead).**
  The writer (`crates/dh-inputlog/src/dhilog.rs:191` `net_rx`) and reader
  (`crates/dh-inputlog/src/reader.rs:534-536`, with an explicit "zero-length accepted by
  design" comment) both accept a 0-length `NET_RX` record, but `apply_net_rx`
  (`crates/dh-devices/src/net.rs:120`) rejects `len == 0` and faults the slot. A legal
  recorded input that cannot be replayed is a determinism hazard. Decide one policy and
  make all three layers agree: either reject 0-length at *record* time (add the lower bound
  to `net_rx` and `validate_kind`, document `>= 1` in API.md §3.3 row `0x03`), or accept it
  on delivery (`apply_net_rx` copies nothing, sets `rx_len = 0`, returns the vector).
  Because the `NET_RX` producer is owned by y78/fbr and not in this bead, track this as its
  own bead (depending on / blocking y78) rather than expanding this device change.

### Suggestions

- [ ] **Rename / re-document the empty-frame rejection.** `apply_net_rx` returns
  `NetRxError::FrameTooBig` for `len == 0` (`net.rs:120`), which is semantically wrong and
  forces an apologetic test comment (`net.rs:341`). Add an `EmptyFrame` variant or drop the
  `len == 0` clause if it is accepted per the Important item above; at minimum update the
  `FrameTooBig` doc comment to cover the empty case. (S-1)

- [ ] **Note the `FrameTooBig`-before-`NoRxBuffer` precedence intent.** With `rx_cap`
  defaulting to 0, a missing buffer surfaces as `FrameTooBig` before the `NoRxBuffer` check
  (`net.rs:120-125`). Fine as internal-only diagnostics, but if `NetRxError` is ever
  operator-visible, reorder the `rx_buf_gpa == 0` check first so the reported cause points
  at the actual guest omission. Add a comment recording the chosen precedence. (S-2)

- [ ] **Confirm/document the status-code divergence from pv-blk.** `PvNet` uses
  `STATUS_IDLE = 0 / OK = 1 / FAULT = 2` (`net.rs:39-41`) vs `PvBlk`'s `STATUS_OK = 0`
  (`blk.rs:50`). The pv-net choice is arguably better; add a one-line module-doc note that
  the extra IDLE state is intentional so it is not "fixed" to match pv-blk later. (S-3)

- [ ] **Document the sticky `TX_STATUS` contract.** `TX_STATUS` is not cleared by
  `TX_BUF_GPA`/`TX_LEN` writes and reflects only the most recent doorbell. Correct and
  deterministic, but add one sentence to the module doc so SDK/driver authors (fbr) do not
  read a stale status before ringing the doorbell. (S-4)

### Informational (no action required)

- `PvNet` is intentionally **not** wired into any bus/engine joint test yet
  (`crates/dh-worker/tests/common/mod.rs` registers PvPad and PvEntropy but not PvNet). Per
  the `mmv` bead's BLOCKS graph, bus/recording integration is owned by `y78` (recording
  integration) and `fbr` (nanokernel loopback guest). Deferring the joint-test bus variant
  to those beads is the correct ownership split, not a gap in this change. When y78 lands,
  it should add a `PvNet` register to the joint capture/restore bus so the NETL section is
  exercised through the real DHSNAP round-trip (the device's own `snapshot_restore_roundtrip`
  test covers the codec; only the engine integration remains).

- The "double doorbell in one segment" determinism question (raised in the review brief)
  is handled correctly *given* y78's per-exit drain contract: because the device buffers
  nothing, a subscriber must read the TX frame from guest RAM at the very exit that rang the
  doorbell, before the guest can overwrite `TX_BUF_GPA`/`TX_LEN` for a second frame. The
  module doc (`net.rs:7-19`) states subscribers re-read "at the very exit that rang the
  doorbell," which implies per-exit draining, but it does not state the *requirement* that
  run control MUST drain per exit (i.e., that a second doorbell before a drain would lose
  the first frame to the subscriber). This is a correctness precondition this device relies
  on but does not own. Consider adding one explicit sentence to the module doc making the
  per-exit-drain dependency a stated contract (e.g., "run control MUST drain the NET_TX
  subscriber on the same exit; a second doorbell before a drain overwrites the live TX regs
  and the first frame is unrecoverable"). This is documentation-only and can fold into the
  I-1 doc pass.
