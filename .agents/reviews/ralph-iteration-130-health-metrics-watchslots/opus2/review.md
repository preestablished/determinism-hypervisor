# Review: ralph/iteration-130-health-metrics-watchslots

Checkpoint: `a213f0c26f0a34e20f49e175ce2ae2a8368c3fba` (`ralph: iteration 130 health metrics watchslots checkpoint`)
Bead: `determinism-hypervisor-6ad`

## Findings

1. High - `WatchSlots` publishes stale `icount`/base data for `Run` completion events.

   `Run` calls `mark_paused` first, which publishes the `Paused` `SlotInfo`, then updates the position with `set_position` afterward (`crates/dh-worker/src/service.rs:2964`-`2977`). `SlotManager::mark_paused` publishes immediately from the current entry (`crates/dh-worker/src/slot_manager.rs:501`-`511`), while `set_position` only mutates `icount`/`base_snapshot_id` and emits no event (`crates/dh-worker/src/slot_manager.rs:517`-`531`). A watcher therefore observes the important `Running -> Paused` transition with the previous boundary position, and never receives the corrected `icount` for that transition. The current test only asserts state (`crates/dh-worker/src/service.rs:4173`-`4181`), so it misses this. Fix by publishing an atomic state+position update for run completion, or update the position before the `mark_paused` event is emitted, then add a WatchSlots test that verifies the event carries the run's final `icount`.

2. Medium - `dh_worker_slot_icount_total` is exported as a Prometheus counter even though the series can reset on slot reuse.

   The metrics renderer declares `dh_worker_slot_icount_total` as `# TYPE ... counter` and emits the current `SlotInfo.icount` keyed only by `slot_id` (`crates/dh-worker/src/service.rs:279`-`289`). But releasing a slot resets the entry to `SlotEntry::empty()` (`crates/dh-worker/src/slot_manager.rs:636`-`639`), whose `icount` is zero (`crates/dh-worker/src/slot_manager.rs:160`-`170`). After `DestroyVm` or lease reclaim, the same Prometheus time series, for example `slot_id="0"`, can drop from a positive value to `0` without a process restart. That violates counter monotonicity and will produce misleading `rate()`/reset behavior. If this metric is the current occupant's boundary, it should be a gauge; if it must be a counter, keep a separate monotonic lifetime total or add a stable generation/lease label and lifecycle tests.

3. Medium - `dh_worker_landing_single_steps_total` is exposed but never incremented.

   The metric exists as an atomic field and is rendered in `/metrics` (`crates/dh-worker/src/service.rs:228`-`237`, `crates/dh-worker/src/service.rs:337`-`347`), but there is no observation path that updates it. The run path records KVM exits (`crates/dh-worker/src/service.rs:2887`-`2889`) but does not receive or record the boundary engine's near-approach single-step count; the actual single-step loop lives in `crates/dh-vmm/src/boundary.rs:151`-`193` without surfacing stats. As a result, the ARCH s9 family is present but semantically dead, so the metrics audit can pass names while the operational signal remains stuck at zero. Fix by returning step/refinement counts from the boundary/run-control path and incrementing this counter in worker run/verification paths, with a test that forces a pure single-step landing and observes a positive counter.

## Residual Risk

I did not run the hardware-gated acceptance suite for this review-only pass. The findings are from static inspection of the checkpoint diff and nearby tests.
