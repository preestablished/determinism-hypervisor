# Tests

No blocking test gaps found for the bead scope.

The new reader validation cases cover:

- unsealed logs are rejected by `LogReader::parse` but inspectable through `LogInspection::parse_unsealed`
- body_hash mismatch is ignored for inspection
- missing END is inspectable as a valid prefix
- END boundary/cross-check failures remain replay errors but not inspection stop conditions
- record-level corruption stops inspection at the valid prefix
- empty `NET_RX` is still rejected by the strict known-kind layout gate
- truncation and single-byte corruption smoke loops exercise both `LogReader` and `LogInspection`

The fuzz target now exercises `LogInspection::parse_unsealed` before the existing sealed `LogReader::parse` path and calls every public record accessor, including `Record::body`, for accepted inspection prefixes.

Validation run:

- `cargo test -p dh-inputlog`
- `cargo +nightly fuzz check dhilog_parse`

