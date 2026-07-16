# Review Overview

- Branch: `ralph/iteration-113-record-replay-corpus-nightly-reverification`
- Date: 2026-06-15
- Reviewer: Local Subagent (2nd reviewer)
- Overall verdict: REQUEST_CHANGES

This branch adds a checked-in pad-echo record/replay corpus, a verifier that reconstructs the root snapshot from fixture bytes and replays the DHILOG against the current engines, and a new `nightly-drift` job that runs the corpus verifier on the locked `kvm-intel` determinism class.

The core replay path is strong: fixture bytes are hashed, the snapshot ref is reconstructed through the real store path, replay verifies epoch hashes and end state, and the reseal byte compare still pins DHILOG bytes. The main gap is in the determinism-class/re-baseline contract. The code and fixture comments say a `ci/determinism-class.lock` bump must re-baseline the corpus in the same reviewed commit, but the corpus manifest does not record the lock identity and the verifier never reads the lock. That leaves the policy as review folklore rather than an executable guard.

## Stats

- Files changed: 7
- Lines added/removed: +466/-4
- Commits: 1
- Commit history: `f915ff6 ralph: iteration 113 checkpoint - record replay corpus`

## Review Context

- Reviewed the committed branch diff `main...HEAD`.
- Read the full changed files:
  - `.github/workflows/nightly-drift.yaml`
  - `crates/dh-worker/tests/m5_record_replay.rs`
  - `crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s/README.md`
  - `crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s/expected.txt`
  - `crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s/recording.dhilog`
  - `crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s/root-sparse.bin`
  - `crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s/root.dhsnap`
- Checked related context in `crates/dh-worker/src/replay_engine.rs`, `crates/dh-worker/src/restore_engine.rs`, `crates/dh-worker/src/snapshot_engine.rs`, `crates/dh-inputlog/src/reader.rs`, `crates/dh-inputlog/src/dhilog.rs`, `ci/check-determinism-class.sh`, and `docs/ops/host-config-intel-box.md`.
- Ran:
  - `bd prime`
  - `git diff --check main...HEAD`
  - `test -c /dev/kvm && test -r /dev/kvm && test -w /dev/kvm`
  - `timeout 180s cargo test -p dh-worker --test m5_record_replay record_replay_corpus_pad_echo_6s_reverifies -- --nocapture`

