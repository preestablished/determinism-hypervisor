# API Safety

No blocking findings.

The API separation is the important safety boundary: inspection returns `LogInspection`, not `LogReader`, and replay/verification call sites found by search still use `LogReader::parse`. `InspectionStop::Eof` is documented as not implying replayability, which is the right wording for an API that intentionally skips final consistency gates.

`Record::body` remains safe for inspection records because `scan_next_record` validates framing, payload bounds, flags, padding, and known-kind layouts before returning a `Record`. Unknown AUX kinds still use the `RecordBody::Unknown` fallback, so they do not require a known layout to avoid panics.

Best-effort corruption behavior is explicit and conservative: inspection returns the prefix of fully validated records and records the first record-level `ReadError` in `InspectionStop::Corrupt`. Header-shape errors still return `Err`, which is consistent with the stated contract.

Minor non-blocking note: the module-level reader comment still says iteration is allocation-free, while `LogInspection` stores inspected records in a `Vec`. The crate already uses allocation elsewhere, and this does not weaken replay safety. It is only a possible future documentation cleanup.
