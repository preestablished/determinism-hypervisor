# Correctness

No blocking findings.

`LogReader::parse` still performs the sealed-log battery: `parse_header` requires `FLAG_SEALED`, the body hash is checked before record validation, and `validate_records` still enforces END presence, END-last, record count, HAS_AUX, EPOCH_HASHES, and END/header cross-checks.

The shared `scan_next_record` preserves the old record-walk ordering for sealed parse:

- short record header and truncated payload checks precede slicing
- payload length is bounded by `MAX_PAYLOAD`
- `seq` must equal the record index
- `icount` must not regress
- unknown record flags and nonzero padding reject the record
- known-kind layouts are validated before `Record::body` can be used

`LogInspection::parse_unsealed` uses the same record scanner but stops at the first record-level corruption and returns only the valid prefix. It intentionally skips the gates named by the bead: SEALED, body_hash, END-present/END-last, and END/header cross-checks. It also skips record_count/HAS_AUX/EPOCH_HASHES consistency, which is consistent with inspecting unsealed or partially written crash artifacts whose seal-time header fields may not have been finalized.

Empty `NET_RX` remains strict. Inspection does not manufacture a typed zero-length frame; it reports `InspectionStop::Corrupt(ReadError::BadPayloadLayout { kind: KIND_NET_RX, seq: 0 })`.

