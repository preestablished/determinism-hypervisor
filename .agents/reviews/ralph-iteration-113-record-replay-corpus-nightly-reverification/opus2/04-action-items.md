# Action Items

## Critical

- None

## Important

- [ ] `crates/dh-worker/tests/m5_record_replay.rs:614` Record the BLAKE3 hash of `ci/determinism-class.lock` in `expected.txt` during corpus regeneration.
- [ ] `crates/dh-worker/tests/m5_record_replay.rs:743` Have `record_replay_corpus_pad_echo_6s_reverifies` read the current `ci/determinism-class.lock` and fail if its hash differs from the corpus manifest.
- [ ] `crates/dh-worker/tests/m5_record_replay.rs:649` Consume every generated `expected.txt` field, including `records_applied`, or remove fields that are not part of the executable oracle.
- [ ] `crates/dh-worker/tests/m5_record_replay.rs:263` Add an allowed-key/required-key check for `expected.txt` so stale keys and stale `epoch_*` entries cannot survive re-baselines.

## Suggestions

- [ ] `crates/dh-worker/tests/m5_record_replay.rs:213` Reject impossible sparse-root `count` values before allocating and validate the encoded byte length up front.
- [ ] `.github/workflows/nightly-drift.yaml:78` Add a tight `timeout-minutes` to the `record-replay-corpus` job.
- [ ] `crates/dh-worker/tests/m5_record_replay.rs:738` Split non-KVM fixture parsing/hash validation from KVM replay so hosted CI can catch malformed corpus files.
- [ ] `crates/dh-worker/tests/m5_record_replay.rs:772` Install the kick handler explicitly in the corpus replay test to match the replay/boundary preconditions.

