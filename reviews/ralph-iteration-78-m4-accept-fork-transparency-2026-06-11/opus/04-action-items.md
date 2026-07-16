# Action items

### Critical

_None._

### Important

_None._

### Suggestions

All optional, non-blocking, documentation/clarity only. Self-contained:

- **S1 — vns 0-based comment (leg 2).** Add a one-line comment in
  `frozen_parent_children_replay_identical_inputs_identically` (near the
  `run_child` closure) noting that `out.vns` and the chain links are
  SEGMENT-RELATIVE (0-based) on the §3.1 reset axis, and that `PvClock::vns_base`
  (the parent's absolute 2M, set by `apply_dhsnap`) only feeds pv-clock MMIO —
  which this guest never reads — so it cannot leak into the hash. Pre-empts the
  "is A == B just an absolute-axis coincidence?" question. (It is not: both are
  cleanly 0-based.)

- **S2 — assert cumulative_icount in leg 2.** Add
  `assert_eq!(outcome.cumulative_icount, PARENT_RUN)` inside `run_child` (or once
  outside) to name the fork-point position as a precondition, matching leg 1's
  `assert_eq!(outcome.cumulative_icount, HALF)`.

- **S3 — assert children resume from the parent's chain link.** Add
  `assert_eq!(outcome.chain.value(), fork_boundary.hash_chain)` in leg 2's
  `run_child` so the "both children resume from the SAME link" invariant is
  asserted at the call site (leg 1 already pins this via
  `outcome.chain.value() == r1.state_hash`).

- **S4 — dedupe the ISR-table read.** Optionally extract a
  `read_timer_table(slot: &SlotVm) -> (u64, Vec<u8>)` helper into
  `tests/common/mod.rs`; the inline read in leg 2 duplicates
  `runctl.rs::idt_guest_tests::read_table`. Low priority while it is a single
  small copy.
