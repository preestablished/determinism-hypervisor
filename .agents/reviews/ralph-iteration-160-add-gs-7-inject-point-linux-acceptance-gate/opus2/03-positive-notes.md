# Positive Notes

- The test uses `m9_linux_ready_snapshot`, which verifies the Linux guest reaches `Ready` through `RunUntil::NextSdkEvent` before taking the READY snapshot.

- The DHILOG PIO parser matches the writer layout: `PIO_ANSWER` data is 8 bytes, with the port at bytes `0..2` and the returned value at bytes `4..8`.

- The replay check uses `ready.ready_snapshot_ref` plus `post_snapshot.input_log_id`, so it is exercising `VerifyReplay` from the READY/root snapshot using the sealed DHILOG rather than an in-memory live plan.

- The no-traffic failure mode is explicit. With sibling guest-sdk/reference-workload still not emitting inject traffic, the ignored gate will fail under `DH_M9_ALLOW_SKIP=0` instead of silently passing.

- The docs command shape is consistent with the existing operator-run Linux gates: it uses `DH_M9_ALLOW_SKIP=0`, `--release`, the exact integration test target, `--ignored`, and `--nocapture`.
