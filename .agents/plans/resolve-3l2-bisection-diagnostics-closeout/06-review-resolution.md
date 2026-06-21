# Review Resolution

Two subagents reviewed the plan:

- Mendel: correctness/completeness against `determinism-hypervisor-3l2`.
- Ampere: Linux/KVM reference-host validation and Beads/Git closeout.

## Accepted Findings

The initial plan was directionally correct, but it needed stronger proof
requirements before another coding agent could safely close a P1 blocked bead.

Accepted changes:

- Tightened the icount range requirement. For epoch divergence, the public
  range must match selected checkpoint coverage. For terminal divergence, the
  lower bound comes from checkpoint coverage and the upper bound is the
  recorded end icount. The plan no longer uses the ambiguous "no narrower than"
  wording.
- Added hard KVM/reference-host preflight:
  `/dev/kvm` readability/writability, `dh-workerd --preflight`, and
  `ci/check-determinism-class.sh`.
- Added `DH_REQUIRE_KVM_TESTS=1` and `-- --nocapture` to service-level
  KVM-backed focused tests so self-skips cannot satisfy closeout.
- Added the positive checkpointed VerifyReplay test:
  `verify_replay_rpc_streams_done_for_bisection_checkpoint_log`.
- Added snapshot comparison field tests for RIP/reg-diff/page evidence:
  `rip_mismatch_produces_postcard_reg_diff`,
  `page_hash_mismatch_reports_page_index`, and
  `page_hash_mismatches_are_limited_to_first_64_indices`.
- Broadened CLI validation from one rendering test to `cargo test -p dh-cli
  bisect`, covering parser/request/rendering behavior.
- Added no-skip Linux fixture and Linux READY gates with staged M9 artifact
  paths, while keeping worker `replay_engine linux_boot_once` as supporting
  VerifyReplay evidence.
- Added `clippy` and `cargo build --workspace` gates when code changes are
  needed.
- Fixed Beads/Git closeout ordering: pull/rebase before final validation,
  commit before the Beads evidence comment, record `git rev-parse HEAD`, then
  close/push Beads and Git.

## Deferred Or Rejected

No review finding changes the selected bead. `determinism-hypervisor-3l2`
remains the right choice because it is local and its dependencies are closed;
`determinism-hypervisor-veu` remains blocked on human access to an upstream
planning tree.
