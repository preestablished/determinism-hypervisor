# Critical And Important Issues

## Critical

No Critical issues found.

## Important

### Important: determinism-class lock changes are not coupled to corpus re-baselines

- File: `crates/dh-worker/tests/m5_record_replay.rs:614`
- File: `crates/dh-worker/tests/m5_record_replay.rs:616`
- File: `crates/dh-worker/tests/m5_record_replay.rs:743`
- File: `crates/dh-worker/tests/m5_record_replay.rs:752`
- File: `crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s/expected.txt:1`
- File: `crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s/expected.txt:18`
- File: `.github/workflows/nightly-drift.yaml:78`
- File: `.github/workflows/nightly-drift.yaml:83`

The new corpus verifier executes the behavioral replay, but it does not make the documented determinism-class coupling executable. `expected_corpus_text` emits only a comment saying to re-baseline with `ci/determinism-class.lock`, and the verifier checks fixture metadata, `machine_config_hash`, snapshot bytes, DHILOG bytes, and replay output. It never reads `ci/determinism-class.lock`, and `expected.txt` has no key that identifies the lock contents used when the corpus was blessed.

That leaves a stale-corpus path open: a deliberate lock bump can merge with old corpus bytes if this narrow 6-second pad-echo recording still happens to replay identically on the new class. The workflow comment says a lock bump "must re-baseline the corpus in the same reviewed commit", and the runbook says the same thing, but the nightly job cannot enforce it. For a determinism product, that is a meaningful re-baseline hazard because the corpus becomes decoupled from the host tuple it is supposed to witness.

Suggested fix: add a manifest key such as `determinism_class_lock_blake3=<hash of ci/determinism-class.lock>` during regeneration, and have `record_replay_corpus_pad_echo_6s_reverifies` read the current lock file and compare the hash before replay. That makes a lock-only bump red until the corpus is explicitly regenerated, even when the runtime behavior has not yet drifted enough for this fixture to fail.

### Important: expected.txt carries fields the verifier never consumes

- File: `crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s/expected.txt:16`
- File: `crates/dh-worker/tests/m5_record_replay.rs:637`
- File: `crates/dh-worker/tests/m5_record_replay.rs:649`
- File: `crates/dh-worker/tests/m5_record_replay.rs:696`
- File: `crates/dh-worker/tests/m5_record_replay.rs:538`

`expected_corpus_text` writes `records_applied=...`, and the committed fixture includes `records_applied=5`, but the verifier never reads that key. The replay count is asserted later as `seconds - 1` inside `replay_once`, so a stale or hand-edited `records_applied` line in `expected.txt` does not fail the corpus test.

This is small by itself, but it is exactly the kind of manifest drift that makes binary corpus re-baselines hard to review over time: reviewers see a field in the expected manifest and assume it is part of the executable oracle. The same pattern will also allow stale extra keys to survive, because `expected_map` only provides lookup-by-key and there is no "all manifest keys were consumed" check.

Suggested fix: either remove `records_applied` from `expected.txt`, or compare `outcome.records_applied` to `expected_u64(expected, "records_applied")` in the corpus test. Prefer the latter, plus an explicit required-key/allowed-key list so stale fields and stale `epoch_*` lines cannot remain after a re-baseline.

