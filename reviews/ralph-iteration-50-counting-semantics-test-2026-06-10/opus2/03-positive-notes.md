# Positive notes

- **The fix is minimal and surgical.** Two call sites, each re-asserting `guest_debug`
  only after a *handled* exit, with an honest comment that re-arming is harmless where the
  trap survived (MMIO reads / PIO). The fix does not touch the counting logic, the
  Overshoot guard, or the far/near split — exactly the right blast radius for a measured
  KVM quirk.

- **Parallel safety is real, not lucky (prompt angle 3).** The kick infrastructure is
  per-thread by design: `KICK_TARGET` is `thread_local`, the PMI is routed via
  `F_OWNER_TID` to `current_tid()`, the counter is `open_for_current_thread`, and each
  test owns an independent VM. `install_kick_handler` is process-wide but idempotent
  (`sigaction` only). I confirmed by running the 73 live-KVM `dh-vmm` unit tests (one
  binary, default parallelism, multiple concurrent VMs in one process) 3x stably, and the
  2-test `counting_semantics` binary 20x in parallel. No serialization config exists
  anywhere and none is needed.

- **The HLT inference is airtight (prompt angle 4).** Each park cycle is `hlt` (exits,
  KVM Hlt) + `jmp .park`. The `jmp` never exits and RIP advances, so it MUST retire (+1);
  the measured per-cycle delta is exactly 1; therefore `hlt` retires 0. There is no way
  for `hlt` to retire 1 and `jmp` 0, because `jmp` cannot retire zero. The `vec![1; 10]`
  assertion is the correct shape (11 raw runs, 10 recorded deltas — the first establishes
  the baseline). The ARCH §3.1 edit promoting HLT into the *measured* set (and keeping PIO
  `IN` as EXPECTED-not-yet-isolated) is accurate.

- **The attribution test is a genuinely strong R2 alarm.** It does not merely count — it
  *classifies every retirement individually* (plain/REP-frozen/REP-retire/CPUID/MMIO-r/
  MMIO-w), asserts the per-class tallies (996/1/1/1/1/255), AND cross-checks the sum
  against `COUNTING_DELTA_AT_OUT_EXITS`, AND replays the full step vector bit-identically
  from a second cold boot. The REP-retire-vs-plain disambiguation via `prev_was_frozen` is
  exactly the §3.2 REP rule, correctly applied. A microcode regression that shifted any
  retirement semantic would trip this loudly — which is the documented intent.

- **The sentinel is honest about why it exists.** The `MMIO_WRITE_SENTINEL` short-circuit
  in the harness synthesizes the boundary from registers *after* the write — keeping the
  trace strictly per-instruction even though the engine's own loops re-arm and span. The
  comment explicitly distinguishes the harness path from the engine path, so the two
  treatments of the same KVM quirk do not get conflated.

- **The regression test names the failure it guards.** Its doc-comment states the exact
  pre-fix symptom ("free-runs ~700 instructions to the park HLT … loud Overshoot"), which
  makes it a real regression sentinel rather than a tautology.

- **ARCH §3.2 re-arm note is well placed** — it sits inside the near-approach
  (single-step) block of the pseudocode where the hazard lives, and correctly scopes the
  trap-eating to MMIO-WRITE only.

- Clean clippy on **both** x86_64 and the aarch64 cross target; the whole `#[cfg]`-to-
  empty pattern for the x86-only test target keeps the cross build honest.
