# Positive notes

### P-1. The proto-mirror cross-pin is exactly the right shape

`stop_reason_u8` (recording.rs:91–99) pins the DHILOG END byte numbering in
`dh-vmm` (which cannot depend on `dh-proto`), and the new
`recording_end_byte_agrees_with_the_proto_mapping` test in
`dh-worker/proto_map.rs:611–628` cross-checks it against
`stop_reason_to_proto` for every variant. A renumbering on either side breaks
one of the two pins loudly, and the local `stop_reason_bytes_mirror_proto_numbers`
test (recording.rs:265–272) catches an accidental edit to the byte map itself.
This is the correct way to keep two crates' wire numbers in lockstep without a
shared dependency — and putting the byte-producing function in `recording.rs`
(next to the seal that consumes it) rather than in `dh-inputlog` is the right
call: `dh-inputlog` takes the byte as an opaque `SealParams.stop_reason: u8`
(dhilog.rs:96–97) and should stay ignorant of the `StopReason` enum.

### P-2. The undrained-IRQ seal refusal is a real invariant, faithfully ported

Refusing to seal a segment with a populated IRQ queue (recording.rs:241–243)
carries forward the m1 template's "a populated queue means a silently dropped
injection" check (m1_acceptance.rs:261–263) into the productized layer, and the
host NET_RX test exercises both halves — drain-then-seal succeeds, and the doc
comment makes clear an undrained seal is refused (recording.rs:286–294,
329–355). Good fidelity. (The variant choice is wrong — see S-1 — but the
invariant is right.)

### P-3. `service_exit` is a faithful, honest port of the m1 `on_exit` body

The serial-PIO / bus-MMIO / loud-log-fault structure (recording.rs:96–132)
matches m1_acceptance.rs:199–245 line-for-line in behavior, including the
`boundary_rip = 0` debug-loop convention and the post-dispatch
`ctx.log_fault()` check that maps a dropped record to a `BoundaryError`
unwind. The module docstring is admirably explicit about what is *dropped* vs
the template — the detcall PIO window and beacon collection are NOT here, with
a clear rationale (DetChannelHost is generic over mem/fault-plan; ol1 owns
production composition; m1 keeps its loop) (recording.rs:17–21). That is the
right thing to omit and the right way to document the omission.

### P-4. The live test is a genuine end-to-end proof, not a smoke test

`pad_echo_live_run_records_inputs_frame_marks_and_seals` (recording.rs:401–547)
proves the load-bearing claims through real KVM execution: PAD_SETs applied at
landed boundaries both (a) reach the guest — all three latch eras appear in the
guest RAM table (recording.rs:511–519) — and (b) land in the sealed log at
those *exact* icounts (`pads == vec![(o1.boundary.icount, ...), ...]`,
recording.rs:532–538); FRAME_MARK AUX records flow from the device during the
MMIO-dense frame loop; and END carries `o3.reason`/`o3.state_hash`. This is the
first production load on the iteration-83 budget-landing fix and it exercises
it across three segments. (Strengthening suggestions in S-5, but the spine is
solid.)

### P-5. The `as_any_mut` overrides follow the established PvClock pattern cleanly

`PvPad` (pad.rs:155–159) and `PvNet` (net.rs:9–13 of the diff) gain minimal,
well-commented `as_any_mut` overrides keyed to the recording layer's downcast
seam, matching the existing `PvClock` precedent the rail's `device_mut` relies
on. No surface area beyond what the rail needs.

### P-6. Pairing-by-construction is the right architectural instinct

Routing canonical inputs through `apply_pad_set` / `apply_net_rx` so the device
mutation and the record landing are a single call (rather than two independent
caller actions) is the correct design to make "applied-but-unrecorded" hard to
express. The push of the returned edge vector onto `self.irqs` in the same
method (recording.rs:167–169, 197–199) keeps the §3.4 injection bookkeeping
paired too. The only gap is that the *atomicity* the design implies isn't fully
real (I-2) and one AUX record type (TIMER_FIRE) isn't routed through a paired
method at all (I-1) — but the instinct and the structure are right.
