# Hypothesis (offered, not asserted)

We are downstream of the worker and cannot see guest serial through
`dh-workerd`, so treat the *mechanism* below as a lead to confirm or refute —
not a diagnosis. The *location*, however, is now pinned: the deciding control
(`00-overview.md`, `01-evidence.md`) showed a **fresh boot** through dh-worker
also `HARD_CAP`s with no frame, so this is **H2** — the dh-worker/dh-vmm
no-tick `Run` frame/drain path, on the **boot path** itself. The
restore-time re-attach (H1) is at most a secondary, additional break; the
primary suspect is the ring-W drain servicing that never fires under no-tick on
any path.

## The signature

Two facts constrain the mechanism:

1. **Instruction count advances** (`10e9` burned) → the guest is *running*, not
   halted / HLT-idle. A workload blocked in `epoll_wait` would burn ~0
   instructions; this burns billions.
2. **Zero GuestEvents are drained** over that burn → the host side never
   consumes a single ring-W record, and the guest never reaches the pv-pad
   `FRAME_COUNTER` MMIO exit.

A running guest that emits nothing and drains nothing is consistent with the
guest being wedged inside its **first** ring-W frame emit — spinning on a
critical ring whose doorbell the worker never answers.

## Mechanism (ring W — corrected; the retry-until-published is confirmed)

The frame path is **ring W** in the guest-**sdk** crate (not the agent's ring
A). Verified anchors in `guest-sdk`:

- `crates/detguest-sdk/src/lib.rs:223` `frame_mark()` (public API; trait impl
  at `lib.rs:507`) publishes a `FrameMark` **ring-W** record and *then* writes
  `FRAME_COUNTER` — ordering asserted by the SDK's own unit test
  `frame_mark_publishes_record_before_frame_counter_write` (`lib.rs:1075`).
- The ring-W record is emitted with `EventClass::Critical`
  (`crates/detguest-sdk/src/channel.rs:127` `emit_w_event` →
  `channel.rs:134` `.emit(..., class, pio::doorbell_w)`).
- `EventClass::Critical` is documented (`channel.rs:154-159`) as
  **"Doorbell and retry until the event is published."** — i.e. a critical
  ring-W emit **spins (doorbell + retry) until the host drains**. (The
  non-critical variant instead drops and accounts the drop; frame marks are
  critical.)
- The doorbell is `DOORBELL_RING_W` (`crates/detguest-sdk/src/pio.rs:82`
  `doorbell_w` → `detcall_out(PORT_DOORBELL, DOORBELL_RING_W)`).

So: if the worker's host-side **ring-W** drain / doorbell servicing is not
active, the restored (or booted) guest's first post-Ready `frame_mark()` rings
`DOORBELL_RING_W`, gets no drain, and **retries forever** — instructions burn,
nothing drains, `FRAME_COUNTER` is never written (it comes *after* the emit),
`frame_budget` is never satisfied. Every observed number fits.

## Where to look (verified anchors in this repo)

- **Primary (H2):** the fresh-boot / `Run` ring-W drain path (`DOORBELL_RING_W`
  → drain) that is supposed to service a running guest — since a fresh boot
  `HARD_CAP`s, verify this servicing fires **at all** under the no-tick config.
  `crates/dh-worker/src/runtime.rs:435` keeps the "Drained detchannel events
  not yet selected by StreamGuestEvents" buffer; if it stays empty post-`Ready`,
  nothing is draining (consistent with the zero-event observation). Compare the
  worker's doorbell/drain wiring to guest-sdk `VmHarness`, which drains ring W
  at the `FRAME_COUNTER` MMIO exit and *does* frame this workload no-tick — the
  delta between the two is the likely fix site.
- **Secondary (restore only):** `crates/dh-worker/src/restore_engine.rs:1-11`
  restores in the §8.3 order **RAM → devices → vCPU** and calls out
  **"DetChannelDevice's EVTC re-attach (detchannel.rs) is the load-bearing
  consumer … the device loop supplies a fresh fault plan and re-attaches."**
  This matters only if restore *additionally* fails to re-arm servicing on top
  of the boot-path break; fix the boot path first.

## Alternatives we have NOT excluded (do not tunnel on the spin)

- This tested exactly one snapshot ref (`1499c0a7…`); a snapshot taken at a bad
  boundary — e.g. mid-emit with the ring-W **producer sequence** restored
  inconsistently (`dh-devices` `detchannel.rs` `restore_producer_seqs`) — is
  not excluded.
- A vCPU-events / device-state restore gap that parks the frame loop **before**
  its first emit would produce the identical "runs, emits nothing" signature —
  the spin is *consistent with* the evidence, not proven by it.
- The `frame_budget` stop wiring itself under no-tick post-restore.

## Why `RestoreSnapshot verification succeeded` did not catch this

The M9 handoff's restore-verify (`crates/dh-worker/src/m9_handoff.rs`) checks
**state-hash equality** at the restored boundary. A hash-faithful restore can
still fail to *resume* forward progress if a **non-hashed, host-side** seam —
the ring-W drain / doorbell servicing wiring — is not active. State equality is
necessary but not sufficient for "the guest makes forward progress." That is
precisely the gap the requested test (`03-ask.md`) closes.
