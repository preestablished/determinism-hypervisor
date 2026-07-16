# Positive Notes

- `crates/dh-devices/src/detchannel.rs:319` cleanly separates `EVTC_V1_LEN` from the v2 base `EVTC_LEN`, which makes the compatibility boundary explicit instead of overloading one constant.
- `crates/dh-devices/src/detchannel.rs:380` uses checked arithmetic when deriving the expected v2 pending-table length, avoiding malformed-section overflow hazards in the device restore path.
- `crates/dh-devices/src/detchannel.rs:396` rejects duplicate or unsorted pending entries by requiring strictly increasing `iseq`, which keeps the canonical serialized map shape deterministic.
- `crates/dh-devices/src/detchannel.rs:665` consumes restored pending injects through a one-shot map and logs the answer through the same `CtxSink` PIO-answer path, preserving the "exactly one answer record per IN" pattern.
- `crates/dh-devices/src/detchannel.rs:1404` adds a focused OUT/restore/IN regression test and verifies that the restored pending state is cleared by the matching `IN`.
- `crates/dh-worker/tests/linux_worker_api.rs:826` updates the acceptance helper to allow both legacy EVTC v1 and current EVTC v2 shapes instead of pinning the Linux worker API test to one device-section version.
