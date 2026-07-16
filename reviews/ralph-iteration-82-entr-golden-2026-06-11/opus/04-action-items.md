# Action Items

### Critical

_None._

### Important

_None._

### Suggestions

- **[S1] Fix the misleading fault-path poison comment in `entropy_draw.asm`.**
  The comment `; poison: harness count check trips` overstates the `LEN=0xDEAD`
  write. `0xDEAD` (57005) is below `MAX_FILL` (1<<20), so it does not trip
  `STATUS_FAULT`; and the harness count assert trips because `.fault_spin`'s HLT
  loop stops the count advancing, **regardless of the LEN write**. Either reword
  the comment to say the marker is for a host inspecting device regs and that the
  count stops due to the fault HLT, or delete the `LEN` write and keep only the
  `.fault_spin` HLT loop. Comment/clarity only; no behavior change.

- **[S2] Optionally note that the STATUS-fault path is device-unit-tested
  elsewhere.** The guest's `.fault` branch is never taken in the happy path this
  acceptance runs. Its behavior is covered by `entropy.rs`'s
  `bad_gpa_faults_without_serving` / `oversized_len_faults`. A one-line comment in
  `entr_golden.rs` pointing there would explain why `.fault` looks "dead" in this
  test.

- **[S3] Optionally strengthen the golden-nonzero pin.** `golden.iter().any(|b|
  *b != 0)` is sufficient; for belt-and-suspenders against a partial-fill bug you
  could also assert the last golden draw is nonzero or that no 16-byte slot is
  all-zero. Low value — the byte-equality assertion already catches structural
  fill bugs.

- **[S4] Optionally hoist the `0xD000_3000` entropy-window base into a shared
  constant.** It is currently a literal in both `crates/dh-worker/tests/common/mod.rs`
  (`bus.register(0xD000_3000, …)`) and `tests/nanokernel/tests/elf_shape.rs`
  (`define("ENT_BASE") == 0xD000_3000`). A shared const would prevent the guest
  base and the registered base from silently desyncing. Not load-bearing (the
  live test would fail if they drifted apart), so future-proofing only.

### Follow-up bead status (informational)

- **`determinism-hypervisor-4a3`** (P1 BUG, OPEN) correctly captures the
  single-step-across-MMIO overshoot that forced the HLT-batch design. The repro
  is concrete (revert to `Until::Goal{poll_period:4096}` on this guest; observed
  4170 vs target 4096, ~74 instrs overshoot) and the diagnosis — that the
  iteration-50 "MMIO-write eats the trap" re-arm in `boundary.rs:168-176` is
  insufficient for this guest's doorbell + device-RAM-write MMIO mix — is
  plausible and consistent with `boundary.rs` (the iter-50 comment is empirical
  from a single earlier guest and never claimed to cover this write shape). The
  ~74-instr overshoot magnitude matches a lost single-step free-running to the
  next exit, not a poll-point off-by-one. The bead is well-scoped (asks for a
  minimal probe to isolate the losing write shape, then a `boundary.rs` fix). No
  action required for this acceptance; dy8 correctly does not depend on landings.
  4a3 should remain open and is appropriately P1 because it blocks safe goal
  polling / pause roll-forward for device-driven guests (M5+).
