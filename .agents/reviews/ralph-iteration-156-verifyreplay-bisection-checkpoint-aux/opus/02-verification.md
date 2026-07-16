# Verification

## Commands Run

- `bd prime`
- `bd show determinism-hypervisor-o2d`
- `git status --short`
- `git branch --show-current`
- `git merge-base main HEAD`
- `git diff --name-status main...HEAD`
- `git diff --stat main...HEAD`
- `git diff --check main...HEAD`
- `git diff --unified=80 main...HEAD -- crates/dh-worker/src/replay_engine.rs`
- `git diff --unified=80 main...HEAD -- crates/dh-worker/src/service.rs`
- `rg -n "VerifyReplay|bisect|checkpoint|aux|reseal|divergence" crates/dh-worker/src/replay_engine.rs crates/dh-worker/src/service.rs`
- `cargo test -p dh-worker replay_engine::tests::reseal_comparison_ignores_only_bisection_checkpoint_aux_records`
- `cargo test -p dh-worker verify_replay_rpc_rejects_invalid_bisection_checkpoint_gap`
- `cargo test -p dh-worker verify_replay_rpc_streams_done_for_bisection_checkpoint_log`
- `cargo test -p dh-worker verify_replay_rpc_streams_bisection_divergence_with_checkpoint_evidence`
- `cargo test -p dh-worker verify_replay_divergence_mapping_is_honest_about_bisection`

## Results

- `git diff --check main...HEAD`: passed.
- `replay_engine::tests::reseal_comparison_ignores_only_bisection_checkpoint_aux_records`: passed.
- `service::tests::verify_replay_rpc_rejects_invalid_bisection_checkpoint_gap`: passed.
- `service::tests::verify_replay_rpc_streams_done_for_bisection_checkpoint_log`: passed.
- `service::tests::verify_replay_rpc_streams_bisection_divergence_with_checkpoint_evidence`: passed.
- `service::tests::verify_replay_divergence_mapping_is_honest_about_bisection`: passed.

I also accidentally ran two incorrect cargo test filters that matched zero tests; those are intentionally not counted as verification.
