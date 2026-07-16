# Positive notes — what is notably right

## P1. The landing is exact and deterministic under live torture, not just unit-green
50 random ascending targets on one guest landed exactly every time, repeated 4x with zero variance.
The two-boots determinism test (`landing_is_deterministic_across_boots_live`) asserts an identical
`(icount, rip, rcx)` tuple — this is the property the whole platform rests on, and it holds live.

## P2. Faithful to the §3.2 pseudocode, with the right ordering of guards
`c > target` (overshoot) is checked *before* `c == target`, so a past target can never be mistaken for
"already there." `d > skid+slack` chooses far vs near exactly as the doc specifies. The far approach
arms once and runs; the near approach parks the period and steps. The implementation reads like the
spec.

## P3. The EINTR / spurious-kick contract from §3.1 is honored on BOTH arms
Both far and near `guard.run()` matches treat `EINTR` as a stop *request*, clear immediate_exit, and
loop to re-read the counter — never assuming the period was reached. This is the subtle correctness
point that §3.1's run.rs header calls out, and the boundary engine gets it right in both places.

## P4. Throttle hazard designed out, not hoped away
The iteration-16 `perf_event_max_sample_rate` hazard is neutralized structurally: `NEVER_FIRES_PERIOD`
is armed before stepping so the tight step loop never re-arms a small period. One arm per far approach.
The module header explains the "O(1) arms per landing" reasoning, and the live tests run thousands of
single steps without tripping the throttle.

## P5. Single-step state can never leak to a caller (risk R10)
The post-loop `if stepping { set_singlestep(false)? }` runs on every exit path, Ok or Err. Combined
with the guest having no vPMU and DR writes faulting, TF is never guest-visible. The `set_singlestep`
helper is symmetric (`control = 0` to disable) and only ever issued after an enable, so no spurious
disable on the never-stepped path.

## P6. KickGuard RAII borrow discipline composes cleanly into the loop
`land_at` registers the guard once and routes every vCPU access (`run`, `get_regs`,
`clear_immediate_exit`, `set_guest_debug`) through it. `get_regs` at the boundary is an immutable
borrow through `DerefMut` — sound while the guard holds the exclusive `&mut`. The kick handler's
raw-pointer aliasing is contained by the guard lifetime (documented in run.rs). The handler-pointer
cannot dangle because the guard owns the borrow for the whole landing.

## P7. Thread-safety is demonstrated, not assumed
The full `dh-vmm` suite (58 tests) passes under cargo's default parallelism, which runs multiple
live-vCPU tests concurrently on different threads, each with its own counter routed via `F_OWNER_TID`.
`install_kick_handler` is process-wide and idempotent (same handler via sigaction). The
`--test-threads=1` run also passes. Per-thread routing works in practice.

## P8. vPMU-disabled (iter-30) is a load-bearing positive for this engine
Because the guest has no vPMU, it cannot perturb the host `INST_RETIRED` counter that the whole
landing depends on. This iter-30 decision is what makes the counter a trustworthy oracle here; worth
keeping linked so a future "re-enable vPMU" change knows it threatens §3.2.

## P9. Future-fit with the M3 scheduler is clean
`agenda::StopPoint.icount` (agenda.rs:75-87) is exactly the absolute, counter-space `target` that
`land_at` consumes. The scheduler loop will be `for point in agenda { land_at(.., point.icount, ..); act(point); }`
with no impedance mismatch. The `on_exit` callback is the right seam for §3.4 injection servicing —
only the HLT/exit policy (S3) needs documenting before that lands.
