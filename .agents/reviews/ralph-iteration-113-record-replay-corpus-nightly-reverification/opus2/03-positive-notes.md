# Positive Notes

- The corpus test reconstructs the root snapshot through `put_snapshot_from_parts` and compares the resulting `SnapshotRef`, so the sparse RAM bytes and DHSNAP blob are not just loose sidecar files.

- The verifier keeps the strong replay guarantees from the existing M5 test: epoch hashes are checked at replay link points, end state is compared, guest-observed pad eras are asserted, and the replay reseal must be byte-identical to the fixture DHILOG.

- Fixture hashes in `expected.txt` are useful review anchors for the three binary files. They make accidental binary churn visible even when Git cannot show a meaningful patch for the fixture bodies.

- The re-baseline helper is guarded by both `#[ignore]` and `DH_WORKER_REGEN_RR_CORPUS=1`, which is the right default posture for a checked-in determinism corpus.

- The nightly workflow keeps the corpus verifier behind the determinism-class check and routes failures through the existing issue-creation job, so the new leg is visible in the same operational channel as host drift and canary failures.

- Local checks passed:
  - `git diff --check main...HEAD`
  - `timeout 180s cargo test -p dh-worker --test m5_record_replay record_replay_corpus_pad_echo_6s_reverifies -- --nocapture`

