# Tests

No blocking findings.

The new tests exercise the core inspection behaviors:
- unsealed logs inspect but do not parse as replayable logs;
- body hash mismatch, missing END, and END/header mismatch are inspection-readable but not replay-safe;
- record-level corruption stops after the valid prefix;
- known-kind layout checks still protect `Record::body`, including the empty `NET_RX` case;
- truncation and single-byte corruption smoke tests now cover both parser entry points.

The fuzz target now exercises `LogInspection::parse_unsealed` accessors and `Record::body` before running the sealed `LogReader::parse` path. `cargo +nightly fuzz check dhilog_parse` passes.

Validation run:
- `cargo test -p dh-inputlog --test reader_validation` passed: 33 tests.
- `cargo test -p dh-inputlog` passed: unit tests, golden tests, reader validation, stop reason mirror, and doc tests.
- `cargo +nightly fuzz check dhilog_parse` passed.
- `git diff --check` passed.

Residual risk is limited to normal fuzzing depth: corruption/truncation totality is smoke-tested and compile-checked under fuzz, but this review did not run a timed fuzz campaign.
