# Positive Notes

---

## P1: The apply-mutation-and-record pairing is the correct safety shape

`apply_pad_set` and `apply_net_rx` mutate the device latch/RX buffer **and** write the DHILOG record in one method, returning the edge vector. As the module doc states, "an applied-but-unrecorded input is a replay divergence by construction" — and the code makes the two truly inseparable from the caller's side. This is exactly the invariant the recording layer needs and it's enforced structurally, not by convention. The host test (`net_rx_application_pairs_record_and_delivery`) proves all three effects (record landed, bytes copied to GPA 0x200, vector queued) in one assertion block.

## P2: The undrained-IRQ-queue seal guard is a genuinely good defensive check

`seal` refuses to seal when `self.irqs` is non-empty (a dropped injection must never seal as a healthy segment). The host test asserts this guard works (drains, then seals successfully) and the live test asserts the disabled-vector case leaves the queue empty. A dropped edge interrupt is precisely the kind of silent divergence that's murder to debug at replay time; failing loud at seal is the right call. (See S5 for the variant-naming nit — the guard itself is excellent.)

## P3: The `stop_reason_u8` proto cross-pin is well-engineered

dh-vmm can't depend on dh-proto, so the END-byte numbering is pinned in `recording.rs::stop_reason_u8` AND cross-checked in `dh-worker/src/proto_map.rs::recording_end_byte_agrees_with_the_proto_mapping`, which iterates every `StopReason` variant and asserts `stop_reason_u8(r) == stop_reason_to_proto(r)`. A renumber on either side breaks one pin loudly. Both the local `stop_reason_bytes_mirror_proto_numbers` unit test and the cross-crate test exist. This is the right way to keep two un-linkable numberings in sync. Verified the variant set (`BudgetReached=1, GoalSatisfied=2, HardCap=4, Paused=5, GuestHalted=6`) matches the `StopReason` enum in `runctl.rs:48-55` exactly — note `=3` is intentionally skipped (matches proto).

## P4: The live 3-segment pad_echo proof is a real end-to-end test, not a tautology

It boots the actual `pad_echo` nanokernel under KVM, runs three real `run_segment` quanta, applies PAD_SET between them, and asserts BOTH halves of the contract: (a) **guest-visible** — the guest's frame table contains all three latch eras (0, 0xA1B2, 0xC3D4), proving the applied input genuinely reached the guest; and (b) **log-side** — exactly 2 canonical PAD_SET records at the *exact* boundary icounts (`o1.boundary.icount`, `o2.boundary.icount`), plus >10 AUX FRAME_MARKs, plus an END whose stop byte and state hash match the outcome. It does NOT re-implement the production logic and assert against itself (the research file's #1 pitfall) — it observes guest memory and parses the sealed bytes through the real `LogReader`. The kvm-usable skip guard is correct for a hardware-gated test.

## P5: Verified FRAME_MARK classification — the live test's canonical/aux split is correct

The live test filters `r.canonical()` for PAD_SET and `r.aux()` for FRAME_MARK. Confirmed against `reader.rs:516-520`: `KIND_FRAME_MARK` is classified AUX, `KIND_PAD_SET`/`KIND_NET_RX` are canonical. So `r.canonical()` yields exactly the 2 PAD_SETs (FRAME_MARKs excluded) and `r.aux()` finds the FRAME_MARKs — both assertions are sound. `rec.icount()` (`reader.rs:140`), `r.end() -> (u8,[u8;32])` (`reader.rs:310`), and `RecordBody::{PadSet,NetRx,FrameMark}` field names all match the test's usage.

## P6: The `VcpuExit::MmioWrite(addr, &data)` synthetic construction is safe public-API usage

Verified `kvm-ioctls-0.24.0/src/ioctls/vcpu.rs:116`: `MmioWrite(u64 /* address */, &'a [u8])`. The host test constructs the variant directly (`VcpuExit::MmioWrite(base + REG_RX_BUF_GPA, &gpa)`) to drive the real `bus.write` dispatch path through `service_exit`. This is a normal public-enum construction over a borrowed slice — no `unsafe`, no transmute, no UB, no FFI-lifetime hazard (the borrow outlives the call trivially). It's the same shape the kvm-ioctls docs themselves show for matching the variant. Clean way to test the dispatch path without a live vCPU.

## P7: The `as_any_mut` downcast seams are correctly scoped and documented

Adding `as_any_mut` to `PvPad`/`PvNet` (overriding the `DetDevice` default that returns `None`) is the right minimal seam for the recording layer to reach the concrete `apply_*` methods keyed by `device_id`. The trait default doc (`lib.rs:62-70`) already warned "override ONLY when ... keep the override consistent with the `device_id` the engine matches on" — and `device_mut`/`apply_net_rx` do key the downcast by id (`DEVICE_ID_PV_PAD`/`DEVICE_ID_PV_NET`), consistent with that contract. `PvClock` already used this seam for restore; the recording layer reuses the same pattern coherently.
