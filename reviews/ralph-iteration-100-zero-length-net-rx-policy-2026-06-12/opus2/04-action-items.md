# Action items

Self-contained checklist for `ralph/iteration-100-zero-length-net-rx-policy` (bead 206).

### Critical

- [ ] None.

### Important

- [ ] **Fix the stale, now-false comment in `crates/dh-devices/src/net.rs:154-157`**
      (inside `PvNet::apply_net_rx`). It still claims "the DHILOG codec accepts empty NET_RX
      records" and "the cross-layer zero-length policy is its own bead… until it lands" — but
      bead 206 *is* that bead and this commit lands it, so the codec no longer accepts empties.
      Replace with text matching the landed invariant, e.g.:
      ```rust
      // len == 0 is also forbidden at the codec since bead 206 (writer
      // returns WriteError::EmptyNetRx; reader validation requires
      // 1..=2048), so a delivered frame is never empty — all three
      // layers agree (see the NetRxError doc above).
      ```
      Leave the guard `if len == 0 || len > MAX_FRAME || len > self.rx_cap` itself unchanged.

### Suggestions

- [ ] **Pin the frame cap at compile time** (`crates/dh-devices/src/net.rs:49-51`). `dh-devices`
      already depends on `dh-inputlog`, so make `MAX_FRAME` derive from the source of truth or add a
      const-assert:
      ```rust
      const _: () = assert!(MAX_FRAME as usize == dh_inputlog::dhilog::MAX_NET_RX_FRAME);
      ```
      Turns the prose "mirrors" claim into a guarantee; guards against a future one-sided cap bump
      re-opening the cross-layer mismatch bead 206 just closed.

- [ ] **Add a forward-looking note to bead `lyu`** (inspection-only entry point for unsealed crash
      artifacts): the reader's `validate_kind` now hard-rejects zero-length NET_RX, which narrows
      best-effort inspection of historical/hostile artifacts. When `parse_unsealed` is built, decide
      explicitly whether codec-validity rules like `1..=2048` are replay invariants (relax in
      inspection mode, report record-with-flag) or structural invariants (still enforce). No code
      change in this branch.

- [ ] **(Optional)** Add one corpus seed to `crates/dh-inputlog/fuzz` containing a sealed log whose
      only NET_RX is exactly 1 byte, to keep the new lower bound reachable through the fuzzed
      accessors. Already covered by the `net_rx_frame_boundaries` unit test, so genuinely optional.
