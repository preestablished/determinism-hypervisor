# pad_echo_6s Record/Replay Corpus

Checked-in corpus for nightly determinism drift detection:

- `root-sparse.bin`: sparse 16 MiB root RAM image, storing only nonzero 4 KiB pages.
- `root.dhsnap`: DHSNAP device blob for the root snapshot.
- `recording.dhilog`: sealed DHILOG for the 6 guest-second pad-echo recording.
- `expected.txt`: expected snapshot ref, fixture hashes, end hash, and epoch chain hashes.

Re-baseline only when `ci/determinism-class.lock` is intentionally bumped or
a reviewed code change intentionally changes the state-hash input contract:

```bash
DH_WORKER_REGEN_RR_CORPUS=1 cargo test -p dh-worker --test m5_record_replay regenerate_record_replay_corpus_pad_echo_6s -- --ignored --nocapture
cargo test -p dh-worker --test m5_record_replay record_replay_corpus_pad_echo_6s_reverifies -- --nocapture
```
