# Positive Notes

- `crates/dh-worker/tests/m5_record_replay.rs:200` - the sparse-root decoder validates magic, memory size, page size, sorted page indices, nonzero sparse pages, and trailing bytes before reconstruction.

- `crates/dh-worker/tests/m5_record_replay.rs:704` - the verifier pins all three fixture byte streams with BLAKE3 before using them. I independently checked the committed bytes match `expected.txt` for `root_sparse_blake3`, `root_dhsnap_blake3`, and `dhilog_blake3`.

- `crates/dh-worker/tests/m5_record_replay.rs:717` - reconstructing via `put_snapshot_from_parts(None, MEM, expand_sparse_root(...), DeviceBlob { ... })` is the right semantic level for a corpus fixture: it proves the checked-in sparse form round-trips to the exact snapshot ref rather than bypassing the store manifest path.

- `crates/dh-worker/tests/m5_record_replay.rs:649` - the DHILOG verifier checks the fixture hash, base snapshot id, machine config hash, clock ratio, record count, end icount, end vns, end state hash, and every epoch hash listed by the log.

- `crates/dh-worker/tests/m5_record_replay.rs:785` - the re-baseline path is ignored and env-gated, with the README documenting the regenerate-then-reverify sequence at `crates/dh-worker/tests/fixtures/record_replay_corpus/pad_echo_6s/README.md:12`.

- `.github/workflows/nightly-drift.yaml:78` - the new corpus job is isolated from the existing canary while still sharing the determinism-class prerequisite and failure alert path.
