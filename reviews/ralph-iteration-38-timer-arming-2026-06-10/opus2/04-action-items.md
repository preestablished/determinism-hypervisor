# Action Items

Each item is self-contained (file paths, the change, and why).

### Critical

None.

### Important

- **[I-1] Make the absolute-vs-relative `vns` contract normative in ARCH §6.2.**
  Edit `.agents/docs/determinism-hypervisor/ARCHITECTURE.md` line 433 (`0x18 TIMER_DEADLINE`). Append a normative sentence stating the deadline is an **absolute** vns value on the continuous time axis (persists across snapshot/restore alongside `vns_base`, §8.1) and that **run control rebases it to segment-relative (`deadline - vns_base`) before the §4 ceil conversion**. Mirror the emphatic absolute-state wording already used for `at_frame` in §6.4 (lines 461-465). *Why:* the rebasing is done by a not-yet-written caller (`dh-cli run` hard-codes `timer: None`); the two code docs (clock.rs:88-96 `armed()` and runctl.rs:101-104 `TimerArm`) already agree that the caller subtracts `vns_base`, but ARCH §6.2 is silent — an implementer wiring bead 40q from the spec alone could pass the absolute deadline straight in, a bug silent on fresh-boot (base 0) and only surfacing after the first restore. Optionally add a named `TimerArm::from_device(absolute, vns_base, vector)` helper in runctl.rs so the rebasing site is greppable and unit-testable.

- **[I-2] Add a stale-agenda / mid-segment re-arm note to bead 40q.**
  `bd update determinism-hypervisor-40q` (or add a note): *"A `TIMER_DEADLINE` write that lands mid-segment changes `PvClock::armed()` after `run_segment` already compiled its agenda from the old value — stale-agenda hazard, latent today (no in-loop MMIO dispatch yet). When this bead wires MMIO dispatch into the run loop, define and implement re-plan-vs-defer semantics (deterministic either way — the write icount is deterministic) and document the chosen timer latency in the guest contract. Coordinate with the M6 scheduler design; sibling to bead 583's deferral-past-next-agenda-point note."* *Why:* the agenda is compiled once from `seg.timer` at entry (runctl.rs:202-224) and is correctly pure; the re-plan decision belongs to the device run loop, not the agenda, and currently no bead captures it.

### Suggestions

- **[S-1]** `crates/dh-vmm/src/runctl.rs:123` — replace `.max(start_icount + 1)` with `.max(start_icount.saturating_add(1))` (or `checked_add(1).ok_or(RunError::ClockOverflow)?`) so the standalone `pub fn timer_to_injection` matches the codebase's u64-edge `checked_*`/`saturating_*` house style (cf. agenda.rs:163-170, clock.rs:84-86). Unreachable in practice; consistency only.

- **[S-2]** Add a `// TODO(583): deferral through the timer chain (budget > deadline, IF=0)` near `armed_timer_fires_and_reports_live` (runctl.rs). The current test deliberately exercises only the no-deferral path; the deferral path through `run_segment` (as opposed to `inject.rs` directly) is uncovered and is appropriately blocked on bead 583's IDT-equipped guest.

- **[S-3]** Add `// INVARIANT: exactly one timer slot; revisit if multi-timer lands` at runctl.rs:203, where the timer is appended as the last injection and matched by index equality. Correct for one-shot/single-timer today; flags the assumption for a future multi-source change.
