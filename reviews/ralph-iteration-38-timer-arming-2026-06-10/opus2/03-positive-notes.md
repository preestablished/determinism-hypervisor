# Positive Notes

### P-1. The live test's no-deferral outcome is genuinely deterministic — 5/5 stable, and provably so

I ran `armed_timer_fires_and_reports_live` 5 times. All 5 PASS, zero skips (`/dev/kvm` is rw), and every run asserts `delivered_icount == DEADLINE` (123_456) with `injections_delivered == 1` and no deferral. The assert is **not** brittle. Why it's deterministic, step by step:

1. The test sets IF in RFLAGS *before* the run (`regs.rflags |= 1 << 9`). But `injectable()` (inject.rs:82-91) reads `kvm_run.if_flag`, a value KVM only refreshes on a **VM exit** — a `set_regs` does not refresh the cached `kvm_run` summary.
2. The landing engine (`land_at`) single-steps to reach the boundary. Landing at the converted boundary T = 123_456 **always involves at least one VM exit at or just before T** (the step/PMI exit during landing). That exit refreshes `kvm_run.if_flag` to reflect the now-set IF. This is the same "step once so kvm_run.if_flag refreshes" trick proven in `inject.rs::open_window_injects_and_delivers_live` (inject.rs:263-271).
3. Because budget == deadline, the agenda merges the injection point and the final stop into **one** `StopPoint` at icount 123_456 (agenda.rs `push` coalesces by icount). At that point `run_segment` processes `point.injections` first (runctl.rs:264-302) — `injectable()` is true (window already open from the fresh exit), so `queue_interrupt` succeeds with **zero** deferral steps and `delivered_icount == current.icount == 123_456 == DEADLINE`.
4. The window-open check is a **pure function of guest state at a deterministic boundary**, so it is identical on every run. There is no wall-clock or scheduling input anywhere on this path.

If the window had been closed at T, the deferral path would single-step forward and `delivered_icount` would be `DEADLINE + k` for a *deterministic* k (same guest → same k, proven by `closed_window_defers_deterministically_live`), so even the deferred outcome would not be non-deterministic — it would just be a different fixed number. The test correctly relies on the landing-exit guarantee to pin k = 0. **No flake risk.**

### P-2. The pending queued vector at the segment boundary IS captured by the state hash — and M4 restore of it exists structurally

The budget == deadline case ends with the vector **queued into KVM (`KVM_INTERRUPT`) but not yet delivered** — it is pending for the next entry that never comes in this segment. This is latent VCPU state, and the question is whether determinism machinery sees it.

It does: `canonical_vcpu_blob` (hash.rs:272-273) serializes `events.interrupt.injected` and `events.interrupt.nr` from `KVM_GET_VCPU_EVENTS` into the state hash. So the final-boundary hash **includes** the pending vector. Two replays both queue the same vector at the same boundary → both hashes equal → replay-identical. ✓ (Positive: the hash is not blind to in-flight injection state, unlike the deliberately-omitted `exception_has_payload`, hash.rs:263-266, which is fine because exceptions are never in flight at a hash boundary.)

For **snapshot/restore** (M4), the chain is also intact: ARCH §8.1 (line ~636) records `KVM_GET_VCPU_EVENTS` in the DHSNAP vCPU blob, and §8.3 (line ~682) restores it in the correct order (`SREGS2 before REGS before VCPU_EVENTS`). So a snapshot taken at this boundary captures the pending vector and a restore re-establishes it. **Caveat worth noting** (not a defect of this iteration): §8.1 also states *"pending agenda MUST be empty — snapshots only at quiescent boundaries with no unconsumed scheduled events; TakeSnapshot fails otherwise."* A KVM-queued-but-undelivered interrupt is VCPU_EVENTS state (captured), distinct from an unconsumed *agenda* entry (forbidden) — the two are reconcilable, but the M4 codec bead should confirm a queued `interrupt.injected` at a quiescent boundary is treated as captured-state, not as a non-empty agenda. The structural note exists; flagging it so M4 verifies the reconciliation explicitly.

### P-3. The conversion is a thin, correct adapter over a well-tested `vt` primitive

`timer_to_injection` (runctl.rs:115-128) does exactly three things: call `icount_for_vns_target` (the ARCH §4 `ceil(T*den/num)` function, vt.rs:48-55, exhaustively property-tested over 20k seeded cases incl. u32 extremes and roundtrip identity), map `None -> ClockOverflow` (no silent saturation — matches the "fault loudly" doctrine), and clamp to `start_icount + 1`. No duplicated math, no re-derivation. The conversion ceil rule matches ARCH §4 line 357 verbatim. The `c21` unit case (deadline 9 vns, 2:1 -> ceil(9/2) = 5 instr) and the clamp case are both asserted in `conversion_follows_the_ceil_rule_and_clamps`.

### P-4. One-shot disarm is correctly factored: the converter never sees deadline 0

`armed()` (clock.rs:97-99) returns `None` for `timer_deadline_vns == 0`, so `TimerArm` is only ever constructed from a `Some`. The §6.2 "write 0 disarms" register semantics are honored at the device, and `timer_to_injection` is never invoked with a zero deadline. The smallest live deadline (1, at 1:1) converts to icount 1 and clamps to `max(1, 0+1) = 1` — a real boundary one instruction after start, never icount 0. Clean separation between device-side disarm and run-control conversion.
