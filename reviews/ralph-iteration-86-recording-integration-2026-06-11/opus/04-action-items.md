# Action items

### Critical

_None._

### Important

- [ ] **Resolve the TIMER_FIRE pairing gap (I-1).** The rail discharges
  PAD_SET / NET_RX / FRAME_MARK / ENTROPY / NET_TX but has no method to land
  the AUX TIMER_FIRE record + disarm the one-shot clock, even though
  `SegmentOutcome.timer_fired` exists for exactly this caller
  (runctl.rs:68–69, 139–141) and `LogWriter::timer_fire` is the writer
  (dhilog.rs:254). Either add `DeviceRail::log_timer_fired(&TimerFired)` that
  writes the record and disarms the clock via the `set_clock_vns_base`
  downcast pattern, OR document in the module docstring that TIMER_FIRE pairing
  is deferred to the run-control wiring bead (39w/4qo) and scope it out of
  y78's text. File: `crates/dh-vmm/src/recording.rs`.

- [ ] **Tighten the pairing-atomicity contract (I-2).** `apply_pad_set` /
  `apply_net_rx` mutate the device before writing the record, so a `Log`
  failure leaves device-vs-log drift. This is sound only because every
  `RecordError` is slot-fatal — but that invariant lives in an enum doc
  comment, not in the apply-method docs or the type. Change "atomically from
  the caller's perspective" (recording.rs:150–152) to state the real contract:
  the mutation precedes the record, the two are not transactional, and the
  caller MUST destroy the slot on any `Err`. Strengthen the `RecordError` doc
  to phrase slot-fatality as a binding caller obligation. File:
  `crates/dh-vmm/src/recording.rs:50–61, 150–201`.

### Suggestions

- [ ] **S-1 — Dedicated error variant for the undrained-IRQ seal refusal.**
  `seal` returns `RecordError::NoDevice("undrained irq queue at seal")`
  (recording.rs:242), but a stranded injection is not a missing/undowncastable
  device. Add `RecordError::UndrainedIrqQueue { pending: usize }` and return it
  instead.

- [ ] **S-2 — De-duplicate the bus-scan/downcast logic.** `device_mut`
  (recording.rs:134–148) and the open-coded scan in `apply_net_rx`
  (recording.rs:182–192) carry two copies of the same lookup, split only
  because `apply_net_rx` needs a concurrent `&mut self.mem` borrow. Factor a
  shared `find_device_raw` helper. Low priority.

- [ ] **S-3 — Surface or justify `drain_net_tx`'s read-fault swallow.** A
  `len != 0` doorbell with an unmapped GPA returns `Ok(None)`, identical to
  "no frame" (recording.rs:217–219). Either return a `RecordError` so the
  loopback caller can fault the slot, or expand the comment to explain why
  dropping is safe here when the apply-path is slot-fatal.

- [ ] **S-4 — Test the pairing failure semantics.** Add a unit test driving a
  `LogWriter` to an `IcountRegressed`/`SeqOverflow` boundary and asserting
  `apply_pad_set` returns `Err(RecordError::Log(..))` after the latch moved,
  pinning the slot-fatal contract from I-2 as a tested invariant.

- [ ] **S-5 — Strengthen the live test and cover the untested seams.**
  (a) Assert FRAME_MARK icounts *bracket* each PAD_SET icount (proving the
  budget landed amid the frame loop, not at a quiescent point);
  (b) assert the latch-era entries appear in table order, not just as a set;
  (c) add at least a host-level test for `drain_net_tx` (publish TX buffer +
  doorbell via synthetic MMIO, assert the frame comes back) and ideally one for
  `set_clock_vns_base` — both currently have zero coverage in this change.
  File: `crates/dh-vmm/src/recording.rs:401–547`.
