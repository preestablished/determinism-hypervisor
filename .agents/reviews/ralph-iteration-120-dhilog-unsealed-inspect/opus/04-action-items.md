# Action Items

Required changes: none.

Optional before merge:

- Update `Record` / `Record::body` rustdoc in `crates/dh-inputlog/src/reader.rs` so it no longer says records only come from `LogReader`; `LogInspection::records()` now also yields validated `Record` values.
- If this API becomes adversarial-facing rather than diagnostic-only, consider making inspection record iteration lazy to avoid allocating one `Record` per accepted prefix record.

