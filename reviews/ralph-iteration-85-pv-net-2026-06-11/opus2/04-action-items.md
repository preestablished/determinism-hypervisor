# Action Items

Self-contained, ordered by severity. File paths are absolute-from-repo-root
within `crates/dh-devices/src/net.rs` unless noted.

### Critical

_None._

### Important

- [ ] **I-1 — Add a public TX-register accessor to unblock y78.**
  `PvNet` exposes only `new()` and `apply_net_rx()`; `tx_buf_gpa` and `tx_len`
  are private. The module doc promises run control will "re-read the frame from
  guest RAM through the still-live TX regs," but there is no API to read them
  (contrast `PvPad::frame_counter()` at `pad.rs:84`). Bead y78 (P0, OPEN,
  depends on mmv) needs these at the doorbell exit to land the loopback
  `NET_RX`. Add, mirroring the pad pattern:
  ```rust
  /// TX descriptor at the doorbell exit; valid when tx_status == STATUS_OK.
  pub fn tx_regs(&self) -> (u64, u32) { (self.tx_buf_gpa, self.tx_len) }
  pub fn tx_status(&self) -> u32 { self.tx_status }
  ```
  Keeps "device buffers no frame" intact. (`net.rs`, near line 169.)

- [ ] **I-2 — Document the `rx_buf_gpa == 0` sentinel (or replace it).**
  `apply_net_rx` (`net.rs:123`) treats GPA 0 as "no RX buffer," but page 0 is
  real guest RAM in this layout, and pv-entropy (`entropy.rs:129`) writes to
  `buf_gpa == 0` without complaint — so the two devices disagree on GPA 0's
  meaning. Either (a) document GPA-0-as-"RX disabled" as a deliberate ABI
  reservation in the module doc + a comment at the check site, noting the
  pv-entropy asymmetry, or (b) if §6.7 needs GPA-0 RX buffers, replace the
  sentinel with a separate `rx_enabled` gate. Prefer (a) unless the spec says
  otherwise.

### Suggestions

- [ ] **S-1 — Reorder `apply_net_rx` checks so `NoRxBuffer` is reachable
  intent-first.** The cap check (`len > rx_cap`) precedes the `rx_buf_gpa == 0`
  check (`net.rs:120-124`), so a fresh device (cap 0) always reports
  `FrameTooBig`, never `NoRxBuffer`. Move the buffer check first for a clearer
  error; update the test comment at `net.rs:379`. Determinism unaffected.

- [ ] **S-2 — Note that a `tx_buf_gpa` in the MMIO hole faults.** Extend the
  doorbell doc (`net.rs:137-141`) to state that an unbacked GPA (incl. the
  device hole) → `MemError` → `STATUS_FAULT`, not just "crossing RAM end."

- [ ] **S-3 — Add an MmioBus dispatch test for PvNet.** No bus registration or
  dispatch test exists yet (cf. `serial.rs:196`). Window math verified by hand:
  PvNet 0xD000_5000 → 0x5000..0x6000, debug-serial mirror 0xD000_6000 →
  0x6000..0x7000, both inside the 0x7000-byte hole, no overlap. A `tests/`
  integration test that registers PvNet at `PV_NET_BASE` and round-trips a
  register guards against future collision regressions.

- [ ] **S-4 — Document RX-overwrite frame-loss semantics.** Two `NET_RX`
  records before the guest clears `RX_LEN` silently overwrite the prior frame
  (deterministic; guest's responsibility to drain). One-line note on
  `apply_net_rx` (`net.rs:152`).
