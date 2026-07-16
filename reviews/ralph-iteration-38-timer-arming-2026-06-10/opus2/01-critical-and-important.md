# Critical & Important Findings

## Critical

None.

---

## Important

### I-1. The absolute-vs-relative `vns` contract is asserted in code but never normative in ARCH §6.2 — the exact seam where future wiring will break

**Where:** `crates/dh-devices/src/clock.rs:88-99` (`armed()` + `vns_base` field doc), `crates/dh-vmm/src/runctl.rs:101-109` (`TimerArm` doc), `.agents/docs/determinism-hypervisor/ARCHITECTURE.md:433` (§6.2 `TIMER_DEADLINE`).

**The seam.** The device stores `timer_deadline_vns` as **absolute guest vns** on the continuous time axis (the same axis `vns_base` shifts across snapshot/restore). `armed()` returns that **absolute** value. `TimerArm.deadline_vns`, by contrast, is documented as **segment-relative**, and `timer_to_injection` feeds it straight into `clock.icount_for_vns_target(...)` — which is correct **only if** the value is already segment-relative. The conversion `absolute -> relative` (subtract the segment's `vns_base`) is performed by **nobody today**: `dh-cli run` (tools/dh-cli/src/run.rs) hard-codes `timer: None` and never reads the device. The first caller that wires `PvClock::armed()` into a `Segment` is where this breaks if the subtraction is forgotten.

**Doc-trail adjudication (the two code docs DO agree — no drift between them):**
- `clock.rs:88-96` `armed()`: *"The deadline is ABSOLUTE guest vns (continuous axis, see `vns_base`); run control converts to a segment-relative icount target via `vt::icount_for_vns_target(deadline - vns_base_of_segment)`."* — caller subtracts `vns_base`. ✓
- `runctl.rs:101-104` `TimerArm`: *"`deadline_vns` is segment-relative here (the caller subtracts the segment's vns base from the device's absolute deadline)."* — confirms the same handoff. ✓

So the two **code** docs are consistent and name the same owner (the wiring caller, between `armed()` and `Segment`). **The gap is ARCH §6.2 line 433**, which says only *"vns deadline; write 0 disarms. One-shot."* — it never states the deadline is **absolute** (vs segment-relative), and never states **who** rebases. Compare §6.4 line 461-465, which is emphatic and normative that `at_frame` is absolute FRAME_COUNTER state persisting across snapshot/restore. The timer deadline deserves the same normative sentence, because it has the identical absolute/relative hazard and the identical snapshot-continuity dependency (`vns_base`).

**Why this matters now:** the subtraction lives in a not-yet-written caller. When bead 40q (M1 device loop) or the M6 scheduler wires it, the implementer will read ARCH §6.2 first, find no statement that the on-device value is absolute, and may pass `armed().0` straight into `TimerArm.deadline_vns` — which is exactly wrong for any segment whose `vns_base != 0` (i.e. every restored segment). The bug would be silent on fresh-boot (base 0) and only surface after the first snapshot/restore, where the deadline fires at the wrong icount or converts to a negative/clamped value.

**Recommendation:** Add one normative sentence to ARCH §6.2 `TIMER_DEADLINE`: *"The deadline is an absolute vns value on the continuous time axis (persists across snapshot/restore alongside `vns_base`, §8.1); run control rebases it to segment-relative (`deadline - vns_base`) before the §4 conversion."* Optionally have the future wiring caller perform the subtraction in a single named helper (e.g. `TimerArm::from_device(absolute_deadline, vns_base, vector)`) so the rebasing site is greppable and unit-testable, rather than an inline `deadline - base` that is easy to drop.

---

### I-2. Mid-segment re-arm / stale-agenda hazard has no bead note on the device-loop or scheduler beads

**Where:** the design follows from `runctl.rs:202-224` (the agenda is compiled **once** from `seg.timer` at segment entry) + `clock.rs:97-104` (`armed()` / `disarm()` are mutable device state a guest write changes).

**The hazard.** The timer is one-shot and read **once** before the agenda compiles. Today there is no MMIO dispatch inside `run_segment` (the device run loop is bead 40q, NotYetWired), so a guest **cannot** re-arm mid-segment — the hazard is latent, not live. But when 40q lands and `TIMER_DEADLINE` writes are serviced inside the run loop, a guest that re-arms (or disarms) the timer **after** the agenda was compiled will have changed `armed()` while the agenda still carries the OLD converted deadline. The compiled agenda is now stale: it will fire the old deadline (or fire a deadline the guest just cancelled), and the new deadline will be missed until the next segment boundary.

**Where the re-plan should live:** this is the device run loop's concern, not the agenda's (the agenda is correctly pure). Two viable designs, both belong to bead 40q's contract with a forward note to M6:
1. **Re-compile on arm-changing MMIO writes:** when an MMIO dispatch changes `armed()`, the run loop aborts the current agenda walk at the next deterministic boundary and re-compiles. Deterministic because the MMIO write is itself at a deterministic icount.
2. **Defer to segment boundary:** treat a mid-segment re-arm as taking effect only at the next segment (simpler, but changes guest-observable timer latency and must be documented in the guest contract).

This is exactly the same class of question already flagged on bead 583's note ("wire the deferral-past-next-agenda-point semantics question ... into the M6 scheduler design"). The re-arm/stale-agenda question is a sibling and currently has **no** bead note.

**Recommendation:** Add a note to bead **40q** (M1 device loop — it owns the MMIO dispatch that makes re-arm possible): *"A `TIMER_DEADLINE` write that lands mid-segment changes `PvClock::armed()` after `run_segment` already compiled the agenda from the old value — stale-agenda hazard. The run loop must define and implement re-plan-vs-defer semantics (deterministic either way, since the write icount is deterministic) and document the chosen latency in the guest contract; coordinate with the M6 scheduler design (cf. bead 583 deferral-past-next-point note)."* Keep `run_segment` agenda-pure as-is.
