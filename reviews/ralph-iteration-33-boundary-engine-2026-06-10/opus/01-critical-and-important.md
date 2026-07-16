# Critical and Important findings

## Critical

None.

The race walk-throughs below confirm the foundation logic is sound; I list them
here (rather than only in positives) because they are the load-bearing
correctness arguments a future maintainer must not break.

### Race walk-through 1: PMI already fired between `counter.read()` and `guard.run()` (far branch) — HANDLED

`boundary.rs:128-139`. Sequence on the far branch:

1. `c = counter.read()` (vCPU out of guest, value stable — counter.rs:90-92).
2. `arm_period(d - skid)` re-arms the overflow period.
3. `guard.run()` enters KVM_RUN.

If the overflow signal for a *previously*-armed period (or this newly-armed
one, racing) is delivered after step 1 but before step 3 lands in guest mode,
the run.rs handler has set `immediate_exit = 1` (run.rs:56-81). KVM_RUN then
returns `EINTR` essentially instantly. The match arm at `boundary.rs:133-137`
treats EINTR as a *request*: `clear_immediate_exit`, fall through, loop re-reads.
On the re-read `d` is unchanged or smaller, so the engine either re-arms (still
far) or transitions to stepping. **No overshoot, no hang, no lost wakeup.** This
is exactly the spurious-kick contract run.rs:13-17 mandates, and it is honored.

### Race walk-through 2: `c` cannot move between read and arm — CONFIRMED

The counter is `exclude_host=1, exclude_hv=1, exclude_idle=1`
(counter.rs:60-62), so it counts ONLY guest-mode retired instructions. Between
`counter.read()` and `guard.run()` the vCPU is out of guest mode (no guest
instruction retires), so `c` is frozen and `arm_period(d - skid)` arms relative
to a value that is still current at KVM_RUN entry. The 18-instruction empirical
skid (run.rs:19-21) sits comfortably inside the 8192 margin.

### Race walk-through 3: stale kick left set BETWEEN `land_at` calls — HANDLED

`KickGuard::register` (run.rs:96-101) does NOT clear `immediate_exit`. So if a
queued RT signal set it after the previous call's last `clear_immediate_exit`,
the FIRST `guard.run()` of the next `land_at` returns EINTR immediately. Both
run arms (far at :133, near at :156) catch EINTR and `clear_immediate_exit`,
then re-read. Benign. The test `kick_before_run_returns_immediately`
(run.rs:212) is the live proof of this exact path.

### `clear_immediate_exit` reachable in BOTH run branches — CONFIRMED

Far: `boundary.rs:136`. Near: `boundary.rs:157`. Both EINTR arms clear before
looping. Good.

### Singlestep dropped on ALL paths incl. errors (R10) — CONFIRMED

The `loop` uses `break Err(...)`/`break Ok(...)` to bind `result`, and the
`if stepping { set_singlestep(false)? }` at `boundary.rs:163-168` runs AFTER the
loop on every exit path — Ok landing, Overshoot, Counter error, Kvm error, and
the `on_exit?`/`arm_period?` early `?` returns... **see the Important finding
below for the one nuance here.**

---

## Important

### I-1: `Margins` (boundary.rs) is a second, unconnected source of truth from `MachineConfig` (config.rs)

`boundary.rs:35-49` defines a `Margins { skid_margin: u64, resync_slack: u64 }`
with defaults 8192/1024. `config.rs:75-82,101-102` ALSO defines
`MachineConfig.skid_margin: u32` / `resync_slack: u32` with the same defaults
(`DEFAULT_SKID_MARGIN = 8192`, `DEFAULT_RESYNC_SLACK = 1024`). There is **no
conversion** between them — `grep` finds no `MachineConfig -> Margins` mapping
and no non-test caller of `land_at` yet.

Why this matters: the bead says margins are MachineConfig material, and §3.2
agrees ("Both are `MachineConfig` fields"). config.rs correctly excludes them
from `machine_config_hash` (verified: `landing_knobs_do_not_fork_identity`
passes, tail encodes `EPOCHS_ON` only — config.rs:289). So the *identity*
contract is already right. But the **plumbing** is absent: when run-control
(§3.3) calls `land_at`, it must translate the operator-configured u32 margins
into the u64 `Margins` the engine consumes. Two risks if this is forgotten:

1. The duplicate defaults silently drift (someone changes one constant, not
   the other) and a host with a tuned `skid_margin` runs the engine at the
   hardcoded 8192 — a latent **Overshoot (R1) hazard** on a host where 8192 is
   too small. The whole point of making it configurable is defeated.
2. The `u32 -> u64` widening is fine, but there is no single place asserting
   "these two are the same knob," so the next reviewer can't see the contract.

**Severity rationale:** Not Critical because `land_at` has no production caller
in this slice (foundation only) and the live defaults are correct, so nothing
is *wrong today*. Important because it is a determinism-foundation knob with a
known R1 failure mode if the wiring is built carelessly later.

**Recommendation:** File a bead (see 04-action-items.md A-1) to add a
`MachineConfig -> Margins` conversion (e.g. `impl From<&MachineConfig> for
Margins`) and have it be the ONLY way run-control constructs `Margins`, with a
test asserting the two default constants agree. Until then, add a doc note on
`Margins` pointing at `MachineConfig::skid_margin` as the operator-facing
source so the duplication is intentional and discoverable.
