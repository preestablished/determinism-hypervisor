# Correctness

No blocking findings.

Replay safety looks preserved. `LogReader::parse` still calls the sealed header parser, still rejects unsealed logs before hashing, still checks `body_hash`, and still runs the full record validation path before constructing a `LogReader` (`crates/dh-inputlog/src/reader.rs:294`).

The inspection parser is intentionally separate from `LogReader`. Its docs explicitly say it skips `SEALED`, `body_hash`, `record_count`, `HAS_AUX`, `EPOCH_HASHES`, END presence/order, and END/header cross-check gates (`crates/dh-inputlog/src/reader.rs:344`). That matches the inspection-only crash-artifact use case and avoids implying replayability.

The skipped final-header gates are covered by tests in the intended direction:
- unsealed artifacts still fail `LogReader::parse` with `NotSealed`, but inspect successfully (`reader_validation.rs:169`);
- mutated body bytes still fail sealed parsing at `BodyHashMismatch`, but inspect records for diagnostics (`reader_validation.rs:185`);
- missing END-at-boundary and END/header mismatch are accepted by inspection as diagnostic prefixes rather than replayable logs (`reader_validation.rs:197`, `reader_validation.rs:203`).

The shared scanner appears behavior-preserving for sealed parsing. The factored `scan_next_record` keeps the old order of framing, payload length, seq, icount, record flag, padding, and known-kind layout checks (`reader.rs:547`). `validate_records` still layers END-last, END cross-checks, record count, HAS_AUX, and EPOCH_HASHES consistency on top (`reader.rs:474`).
