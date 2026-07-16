# 03-positive-notes.md
- `crates/dh-inputlog/src/dhilog.rs:337`: The writer API keeps the new record narrowly scoped and preserves existing record ordering and AUX flag behavior.
- `crates/dh-inputlog/src/reader.rs:667`: The reader validates payload length, nested format version, and reserved flags before exposing the typed body.
- `crates/dh-inputlog/tests/reader_validation.rs:762`: Negative tests cover malformed length, unsupported version, and invalid flags.
- `crates/dh-inputlog/tests/reader_validation.rs:831`: The hand-framed bytes test helps catch writer/reader offset drift for the new payload.
