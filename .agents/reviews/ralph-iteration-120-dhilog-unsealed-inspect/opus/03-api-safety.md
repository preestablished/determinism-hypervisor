# API Safety

No blocking replay-safety finding.

Replay type separation is preserved at the entry-point level. `LogInspection` is a separate public type, exposes no `canonical`, `aux`, or `end` replay helpers, and there is no conversion from `LogInspection` into `LogReader`. Current replay and verify call sites still accept log bytes and call `LogReader::parse` internally, so an unsealed inspection result cannot reach the product replay path without new, explicit code.

The inspection API returns the same `Record` type as `LogReader`, but only after the shared scanner validates framing, flags, padding, monotonic watermarks, sequence numbers, and known-kind payload layouts. That keeps `Record::body` infallible for inspection records as well.

Non-blocking API hygiene: the `Record` and `Record::body` rustdoc still describe `LogReader` as the only construction path. That was true before this branch, but `LogInspection::records()` now also yields `Record`. The implementation is safe because inspection validates the same per-record layouts, but the docs should be updated to mention both sealed-reader records and validated inspection-prefix records.

Residual consideration: `LogInspection` stores accepted prefix records in a `Vec`. That is reasonable for current crash-artifact diagnostics, but a future adversarial-facing inspector may want a lazy prefix iterator to avoid allocation proportional to the accepted record count.

