# Positive notes

This is high-quality, determinism-grade work. The hard parts are right.

1. **The REP rule is implemented exactly, not approximated.** The counter
   re-read at the loop top (`boundary.rs:108`) is the SOLE progress signal;
   `Ok(VcpuExit::Debug(_)) => {}` (`:154`) deliberately does nothing on a debug
   trap and lets the next iteration's re-read decide. No `+1` assumption
   anywhere. A boundary is declared only when `c == target` AND we are at an
   instruction start (which, per the module invariant, the counter equality
   already guarantees because mid-REP and mid-emulation never move it). This is
   precisely the §3.2 "if RIP unchanged, continue stepping without counting a
   boundary" rule expressed structurally.

2. **EINTR is treated as a request, never an assertion** — on both the far
   (`:133`) and near (`:156`) branches. This honors the run.rs:13-17
   spurious-kick contract to the letter, and the loop always re-reads the
   counter to decide. The `kick_before_run_returns_immediately` test proves the
   exact "EINTR with counter far from target" case the contract warns about.

3. **Overshoot is loud and unabsorbed.** `c > target` is `break
   Err(Overshoot)` at the very top of the loop (`:109-111`) — checked before
   any arming or stepping, so a stale/past target can never be silently
   approached. The `stale_target_is_a_loud_overshoot_live` test confirms it
   with the exact `{ target: 10_000, counted: 50_000 }` tuple.

4. **Single-step is dropped on ALL exit paths**, including every error path,
   via the post-loop `if stepping { set_singlestep(false)? }` (`:163-168`). The
   `loop { break ... }` pattern guarantees the cleanup runs no matter how the
   loop terminates. R10 (TF never guest-visible) is structurally enforced.

5. **The throttle hazard is engineered out.** At most one real `arm_period` per
   far approach (guarded by `!stepping`, `:126`), and the period is parked at
   `NEVER_FIRES_PERIOD` exactly once on entry to stepping (`:141-150`) so the
   tight step loop never re-arms. This matches the iteration-16
   `perf_event_max_sample_rate` empiric and the module's O(1)-arms claim.

6. **The PMU never goes blind.** The counter STAYS enabled while stepping (the
   period is parked, not the event disabled), so `counter.read()` at the loop
   top keeps returning truth across the far→near transition. Matches §3.2
   ("the PMI counter stays enabled").

7. **`d` monotonically shrinks, and the `!stepping` latch is correct.** Once
   `stepping = true` the engine can never fall back to the far branch within a
   single `land_at` — and it shouldn't, because `d` only decreases (the counter
   only counts up, target is fixed). A spurious EINTR in stepping mode clears
   and continues stepping. There is no path that re-arms a real period mid-step.

8. **Counter revocation surfaces loudly.** `counter.read()?` maps `NotPinned`
   (counter.rs:103/116 — zero-byte read or enabled-but-not-running) straight to
   `BoundaryError::Counter`, breaking the loop. A revoked counter (NMI watchdog
   flips on, PMU oversubscribed) becomes a hard error, never a silent wrong
   landing. Exactly the §7.4 safety posture.

9. **The tests are real and ran.** On this box (KVM rw, paranoid=1) all four
   live tests executed (no "skipping" branch taken) and passed first-run.
   `landing_is_deterministic_across_boots_live` boots TWO independent rigs
   (counter reset per rig via `landing_rig`) and asserts the FULL `(icount, rip,
   rcx)` tuple is bit-identical — this is the determinism foundation's headline
   property, demonstrated live. `lands_exactly_via_pmi_then_step_live`
   additionally proves cross-call composition (1M then 2.5M). `cargo clippy -p
   dh-vmm` is clean.
