# Critical And Important Findings

No critical findings.

No important findings.

## Acceptance Notes

- No-divergence checkpoint recording path: covered by `service::tests::verify_replay_rpc_streams_done_for_bisection_checkpoint_log`, which records with `BisectionCheckpointConfig::every_epoch()`, asserts checkpoint AUX evidence is present, then verifies replay with `bisect_on_divergence=true` and expects `Done`.
- True-divergence checkpoint evidence path: covered by `service::tests::verify_replay_rpc_streams_bisection_divergence_with_checkpoint_evidence`, which records with bisection checkpoints and asserts a refined `Divergence` with `evidence_mode=replay-vs-recorded` and checkpoint/probe refs.
- Reseal normalization path: covered by `replay_engine::tests::reseal_comparison_ignores_only_bisection_checkpoint_aux_records`, including a canonical-record mismatch negative case.

## Low-Risk Observation

`reseal_equivalent_ignoring_bisection_checkpoints` relaxes when either the recorded input or replayed output contains a bisection checkpoint record. Today replay does not emit those records, so this is not a current behavioral bug. If replay ever gains checkpoint emission, a stricter recorded-side predicate may be worth considering so plain input logs still fail on unexpected replay-only diagnostic AUX.
