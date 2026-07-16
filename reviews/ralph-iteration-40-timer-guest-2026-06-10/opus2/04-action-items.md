# Action Items

Self-contained; reference IDs map to 01/02. None block merge.

### Critical

_None._

### Important

- **[I-1] Document `step_one_entry`'s "one entry" precondition.** In
  `crates/dh-vmm/src/boundary.rs` (the `step_one_entry` doc-comment, ~line 9-18), add a line
  stating that `on_exit` MUST return `Err` for any exit that would resume past an instruction
  boundary — notably `Hlt` — otherwise a single call runs more than one logical entry. It is
  safe today only because `run_segment`'s `exits!()` wrapper turns `Hlt` into an `Err`; a
  future caller (e.g. the bead-40q device run loop servicing HLT as idle) would silently
  overshoot. Doc-only change.

- **[I-2] Add a debug-assert that `step_one_entry` made forward progress.** Thread the
  pre-entry icount into `step_one_entry` and `debug_assert!(icount > entry_icount, …)` before
  returning the `Boundary`. This keeps the engine's "never declare an unverified boundary"
  invariant auditable at the one place it is relaxed (no `c > target` overshoot guard, by
  design). Not a bug today (only same-boundary callers exist; M6 unwired), so downgrade to a
  Suggestion if the team prefers to keep the function strictly target-free.

### Suggestions

- **[S-1] Leave `INJECT_DEFER_BUDGET` as a const; add a migration breadcrumb.** The const is
  correct (it bounds only a loud failure, never the success path, so determinism is preserved).
  In its doc-comment at `crates/dh-vmm/src/runctl.rs:26-29`, note the intended migration trigger:
  *promote to `MachineConfig` if bead-40q's `arm` mode legitimately needs ops to tune the
  ceiling per machine.*

- **[S-2] File a bead-40q design note for the `arm`-mode MMIO exit storm.** Once `CLOCK_BASE`
  is a live MMIO region, the `.wait` poll loop in `tests/nanokernel/asm/timer_guest.asm:100-103`
  exits on every iteration — a tight spin between 1ms deadlines that can blow the §10
  ~3k-exits/s envelope. The guest cannot `hlt`/`pause` between polls without the very timer
  interrupt M3 is bootstrapping. Record the resolution options: host-side `pause`/`mwait`
  throttle, or host-synthesized waits (advance vns, re-enter only at the deadline boundary).

- **[S-3] Note that `masked_variant_defers_forever_live` cost tracks `INJECT_DEFER_BUDGET`.**
  It burns the full 65536-step budget per run. Either document that, or expose a test-only
  smaller-budget path. Sub-second today; only matters if S-1's budget is later raised.

- **[S-4] Bound `read_table`'s count read.** The test helper reads `count` bytes unbounded;
  add a `count.min(cap)` guard so a misbehaving future guest can't trigger a huge alloc.
  Cosmetic, test-only.

- **[S-5] One-word comment polish on the gate attribute byte.** Add "type=0xE" to the
  `0x8E00` comment in `SETGATE` (`tests/nanokernel/asm/timer_guest.asm:38`) so the decode is
  grep-complete. Trivial.
