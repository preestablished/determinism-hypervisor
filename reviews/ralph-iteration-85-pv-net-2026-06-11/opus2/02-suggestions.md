# Suggestions

### S-1. `NoRxBuffer` is masked by the cap check in the fresh-device path

**File:** `crates/dh-devices/src/net.rs:120-124`

```rust
if len == 0 || len > MAX_FRAME || len > self.rx_cap {
    return Err(NetRxError::FrameTooBig);
}
if self.rx_buf_gpa == 0 {
    return Err(NetRxError::NoRxBuffer);
}
```

The cap check runs *before* the buffer check. A fresh device has `rx_cap == 0`,
so any non-empty frame returns `FrameTooBig` — never reaching the
`NoRxBuffer` branch. The test at lines 377-380 even bakes this in:

```rust
dev.apply_net_rx(&[1, 2, 3], &mut mem),
Err(NetRxError::FrameTooBig) // cap is 0 — too big before buffer check
```

So `NoRxBuffer` is only reachable in the narrow case where the guest set
`rx_cap > 0` but left `rx_buf_gpa == 0`. The most intuitive "no buffer
published" scenario (everything default) reports the wrong error variant.

**Suggestion:** order the checks intent-first — check `rx_buf_gpa == 0`
(NoRxBuffer) before the size check, so the diagnostic matches what actually
went wrong. Determinism is unaffected (the function is pure over its inputs
either way); this is purely a clearer error contract. If you reorder, update
the test comment accordingly.

---

### S-2. Document the TX_BUF_GPA-into-MMIO-hole behavior

**File:** `crates/dh-devices/src/net.rs:137-141`

`doorbell` reads `tx_len` bytes at `tx_buf_gpa` via `ctx.mem.read`. The
`VecGuestMem` test impl bounds-checks against the Vec and returns `MemError`;
the live `VmMem`/`GuestMemoryMmap` `read_slice` of an unbacked GPA (e.g. a
pointer into the MMIO hole at 0xD000_0000+) also errors. Both map to
`STATUS_FAULT`, which `tx_faults_are_loud_and_logged_nothing` covers for an
unmapped high GPA (0xFFFF_0000).

The behavior is correct, but the doc comment only mentions "MAX_FRAME crossing
guest RAM end." A one-line note that a `tx_buf_gpa` pointing **into the device
MMIO hole** (or any unbacked region) likewise faults — because no memslot backs
the hole — would make the safety argument explicit and pre-empt the obvious
adversarial question.

---

### S-3. Add an integration-level MmioBus dispatch test for PvNet

**Observation:** `PvNet` is not registered on any `MmioBus` anywhere yet
(grep for `0xD000_5000` / `PvNet` outside `net.rs` is empty), and there is no
bus-dispatch test analogous to `serial.rs:196`
(`mmio_mirror_serves_registers_in_4_byte_slots`, which registers DebugSerial at
0xD000_6000 and reads through the bus).

I verified the window math by hand: the §2.2 hole is `MMIO_HOLE_BASE
0xD000_0000` + `MMIO_HOLE_LEN 0x7000`. PvNet at 0xD000_5000 occupies
0x5000..0x6000 within the hole; the debug-serial mirror at 0xD000_6000
occupies 0x6000..0x7000 (the last window). **No overlap, both fit.** ✓

Still, an integration test that `bus.register(PV_NET_BASE, Box::new(PvNet::new()))`
and round-trips a register read/write through the bus would (a) catch a future
base-address collision regression, and (b) match the pattern the research file
recommends (exercise the public bus contract, not just the device internals).
Per `~/.claude/research/rust-integration-testing.md`, this belongs in
`tests/` against the public API. Low priority — the unit tests already cover the
device behavior — but it closes the "registered on the bus at the right window"
gap that only y78 currently exercises.

---

### S-4. RX-overwrite (frame loss) semantics deserve a one-line doc

**File:** `crates/dh-devices/src/net.rs:152-168`

If two `NET_RX` records land back-to-back before the guest clears `RX_LEN`, the
second `apply_net_rx` overwrites the RX buffer and `rx_len` silently — the
first frame is lost. This is **deterministic** (identical in record and replay,
since the record stream is the single source of truth), so it is not a
correctness bug. But silent frame loss is a behavior the guest author needs to
know about ("you must drain RX_LEN before the next delivery, or you drop
frames"). A one-line note on `apply_net_rx` stating that re-delivery before the
guest acks overwrites the prior frame (deterministically; the guest's
responsibility to drain) would document the contract. Consistent with how the
module already documents the "guest clears RX_LEN" ack at the write path.
