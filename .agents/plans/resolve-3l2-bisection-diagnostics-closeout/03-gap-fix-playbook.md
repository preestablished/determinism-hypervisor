# Gap Fix Playbook

This plan expects the audit to pass with little or no code change. If it does
not, use the narrow fix paths below.

## Missing Checkpoint Index Or Validation

Files:

- `crates/dh-worker/src/bisection_index.rs`
- `crates/dh-worker/src/service.rs`

Required behavior:

- Reject unsupported checkpoint format versions and flags.
- Reject sequence-order inconsistencies, checkpoint records separated from
  same-icount epoch hashes by canonical records, too-narrow `max_covered_gap`,
  and unusable snapshot refs.
- Validate DHILOG header identity before dereferencing checkpoint snapshot refs.

Tests to add or strengthen:

- `BisectionCheckpointIndex` unit tests for the missing validation branch.
- Service test proving header mismatch wins before checkpoint-ref validation.

## Missing Or Fabricated Divergence Fields

Files:

- `crates/dh-worker/src/snapshot_compare.rs`
- `crates/dh-worker/src/replay_engine.rs`
- `crates/dh-worker/src/service.rs`

Required behavior:

- Expected snapshot must be the recorded checkpoint snapshot ref.
- Actual snapshot must be captured from replay at the selected probe point.
- `rip_expected`, `rip_actual`, `reg_diff`, and `diff_page_idx` must be copied
  from snapshot comparison output.
- For epoch divergence, `icount_lo`/`icount_hi` must match the selected
  checkpoint coverage. For terminal divergence, the lower bound must come from
  checkpoint coverage and the upper bound must be the recorded end icount. Do
  not widen or narrow the public range beyond the evidence.

Tests to add or strengthen:

- A memory-only divergence test with equal RIP but non-empty
  postcard-encoded empty `Vec<RegDiff>` and non-empty `diff_page_idx`.
- A LAPIC/register divergence test proving `reg_diff` contains the expected
  named register/device diff.
- A wide-gap test proving the public range expands to the actual checkpoint
  evidence window instead of the old fabricated 1024-instruction window.

## Coarse Divergence Still Escapes Under Bisection

Files:

- `crates/dh-worker/src/verify_replay.rs`
- `crates/dh-worker/src/service.rs`

Required behavior:

- `VerifyProgress::Divergence` is public only when bisection was not requested.
- `bisect_on_divergence` defaults to true at the service surface.
- `bisect=false` remains a supported explicit escape hatch for coarse
  diagnostics.

Tests to add or strengthen:

- One service test with `bisect_on_divergence: Some(false)`.
- One service test with `bisect_on_divergence: Some(true)`.
- One service test with `bisect_on_divergence: None`.

## Recorder Does Not Emit Checkpoints In The Right Runs

Files:

- `crates/dh-worker/src/service.rs`
- `crates/dh-worker/src/snapshot_engine.rs`
- `crates/dh-worker/src/runtime.rs`

Required behavior:

- The private `BisectionCheckpointConfig::every_epoch()` path emits
  checkpoints for eligible epoch boundaries.
- Checkpoint capture is non-mutating: no dirty tracking clear, no entropy
  reseed, no public snapshot lineage change, and no extra canonical records.
- The runtime resets checkpoint anchor state on public snapshot boundaries.

Tests to add or strengthen:

- Enabled-vs-disabled checkpoint run equivalence.
- Capture lineage/log-surface preservation.
- A service VerifyReplay test that creates a checkpointed log from the normal
  recorder path and then uses it for bisection.

## CLI Surface Missing

Files:

- `tools/dh-cli/src/ops.rs`
- `tools/dh-cli/tests/cli_args.rs`
- `tools/dh-cli/tests/*`

Required behavior:

- `dh-cli verify` accepts `--bisect` and `--no-bisect`.
- `dh-cli replay` rejects verify-only bisection flags.
- JSON and human output include refined divergence fields and provenance.

Tests to add or strengthen:

- Parser coverage for flags and conflict rejection.
- Rendering coverage for `icount_lo`, `icount_hi`, RIP fields, `diff_page_idx`,
  and suspected cause.

## Do Not Do This

- Do not return fabricated 1024-instruction bisection windows.
- Do not populate evidence fields from coarse epoch hashes alone.
- Do not treat missing checkpoint evidence as success for
  `bisect_on_divergence=true`.
- Do not close `3l2` solely because all child beads are closed; verify current
  code and tests.
