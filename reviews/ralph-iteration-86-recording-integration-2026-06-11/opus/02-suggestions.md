# Suggestions

### S-1. `seal`'s undrained-IRQ refusal returns the wrong error variant

`crates/dh-vmm/src/recording.rs:241–243`.

```rust
if !self.irqs.is_empty() {
    return Err(RecordError::NoDevice("undrained irq queue at seal"));
}
```

`RecordError::NoDevice` is documented as *"The bus has no device with the id
the input targets, or it does not downcast to the expected concrete type"*
(recording.rs:58–60). An undrained IRQ queue is neither — it is a run-control
sequencing fault (a dropped injection). Overloading `NoDevice` here means a
caller matching on the error class cannot distinguish "your bus is missing
pv-pad" from "you forgot to drain the queue before sealing," and the `&'static
str` payload is the only signal — fine for a log line, wrong for programmatic
handling.

**Recommendation:** add a dedicated variant, e.g.:

```rust
/// `seal` was called with vectors still queued — a dropped injection.
/// Drain into the next segment's injection set before sealing.
UndrainedIrqQueue { pending: usize },
```

and return `RecordError::UndrainedIrqQueue { pending: self.irqs.len() }`. This
mirrors the m1 template, which treats a populated queue as its own distinct
failure mode (m1_acceptance.rs:261–263) rather than folding it into a device
error.

---

### S-2. `device_mut` and `apply_net_rx`'s inlined lookup duplicate the bus scan

`crates/dh-vmm/src/recording.rs:134–148` (`device_mut`) and `182–192`
(the open-coded scan inside `apply_net_rx`).

`apply_net_rx` cannot call `device_mut` because it needs `&mut self.mem`
borrowed simultaneously with the device — the comment explains the
split-borrow dance (recording.rs:182–183). Reasonable. But the result is two
copies of the "scan `bus.devices_mut()`, match `device_id`, `as_any_mut` +
`downcast_mut`" logic, which will drift if the bus iteration or downcast
pattern ever changes.

**Recommendation (optional):** factor the lookup into a helper that returns the
`&mut dyn DetDevice` (or splits the bus borrow from the mem borrow more
explicitly), e.g. a `fn find_device_raw(&mut self, id) -> Option<&mut dyn
DetDevice>` that both paths use, with the downcast applied at the call site.
Low priority — the duplication is small and well-commented — but it removes a
latent maintenance trap.

---

### S-3. `drain_net_tx` silently swallows a guest-RAM read fault as "nothing sent"

`crates/dh-vmm/src/recording.rs:208–221`.

```rust
if self.mem.read(gpa, &mut frame).is_err() {
    return Ok(None); // the doorbell already faulted; nothing sent
}
```

A `len != 0` TX doorbell whose buffer GPA does not map returns `Ok(None)` —
indistinguishable from `len == 0` ("no frame"). The comment asserts "the
doorbell already faulted; nothing sent," but a guest that publishes a non-zero
`tx_len` pointing at an unmapped GPA is a guest bug / hostile input, and the
loopback caller will see "no frame to land" rather than a fault. For a
recording layer whose entire job is fidelity, an unreadable-but-claimed TX
frame is arguably worth surfacing.

**Recommendation:** consider returning a `RecordError` (or at least a distinct
`Result` arm) for the `len != 0 && read fails` case, so the loopback caller can
decide whether to fault the slot. If `Ok(None)` is genuinely the intended
loopback semantics (drop and continue), keep it but expand the comment to say
*why* dropping is safe here (vs. the apply-path's slot-fatal stance) — the two
policies are currently inconsistent without explanation. Note this seam is
**not exercised by any test in this change** (see S-5).

---

### S-4. Add a test pinning the pairing failure semantics (ties to I-2)

There is no test that a `Log` failure mid-`apply_pad_set` is observable as an
error and that the slot-fatal contract is the documented escape. A cheap
regression guard: drive a `LogWriter` to an `IcountRegressed` (apply a PAD_SET
at an icount below the last record) or `SeqOverflow` boundary and assert
`apply_pad_set` returns `Err(RecordError::Log(..))` *after* the latch moved —
documenting in the test the invariant that the caller must now kill the slot.
This makes the non-atomicity (I-2) an explicit, tested contract rather than a
latent surprise.

---

### S-5. The live `pad_echo` test's strongest assertions are about the log; the brief's "budget landings inside the MMIO-dense frame loop" claim is only weakly pinned

`crates/dh-vmm/src/recording.rs:401–547`.

The test is genuinely good (see positive notes), but two assertions are softer
than they could be:

- **FRAME_MARK count** `assert!(frame_marks > 10)` (recording.rs:543) proves
  marks flow, but does not pin their icount spread *inside* the budget windows
  — i.e. it does not directly demonstrate the iteration-83 landing fix handling
  a budget boundary that lands amid the MMIO-dense frame loop. The PAD_SETs
  landing at `o1.boundary.icount` / `o2.boundary.icount` (recording.rs:532–538)
  do implicitly prove landing precision, but a stronger check would assert at
  least one FRAME_MARK has an icount `<` each PAD_SET's icount and at least one
  `>` it (marks bracket the landed boundary), proving the boundary landed
  *between* frame writes rather than at a quiescent point.

- **Latch-era assertion strength** (recording.rs:511–519): asserting the
  `seen` set *contains* 0, 0xA1B2, 0xC3D4 is correct but does not assert
  *ordering* (era 0 entries precede 0xA1B2 entries precede 0xC3D4 entries in
  the table). Since the inputs are applied at segment boundaries in sequence,
  the table should be era-ordered; asserting that would catch a "latch applied
  too early/late" bug that the set-membership check passes through.

Neither is tautological — both assertions test real guest-visible state. These
are strengthening suggestions, not defects.

**Also note** `drain_net_tx` and `set_clock_vns_base` have **no test coverage**
in this change (the host NET_RX test covers `apply_net_rx`/`seal`; the live
test covers `apply_pad_set`/`service_exit`/FRAME_MARK/`seal`). `drain_net_tx`
is the §6.7 loopback seam y78 names — at minimum a host-level unit test
(publish a TX buffer via synthetic MMIO, ring the doorbell, assert
`drain_net_tx` returns the frame) would prove the seam before 39w depends on
it.
