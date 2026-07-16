# Critical And Important

No Critical findings.

## Important

Severity: Important
File: `crates/dh-worker/tests/m6_full_api_uds.rs:172`

Problem: The hardware gate returns `None` for missing KVM, unreadable CPU topology, or unavailable slot cores, and the test body returns successfully at `crates/dh-worker/tests/m6_full_api_uds.rs:504`. That means the explicit acceptance invocation can pass without restoring or running any slots. I confirmed this locally with `cargo test -p dh-worker --test m6_full_api_uds -- --ignored --nocapture`: it printed that cores 2-65 were unavailable and still reported `test ... ok`.

Suggested fix: Treat unmet M6 acceptance prerequisites as a test failure when the ignored acceptance test is explicitly run. Replace the `Option` skip path with `Result<Vec<u32>, String>` plus `panic!`/`expect`, or allow skipping only behind an explicit non-acceptance escape hatch such as `DH_M6_ACCEPT_ALLOW_SKIP=1`.

Severity: Important
File: `crates/dh-worker/tests/m6_full_api_uds.rs:318`

Problem: Restored slots are only destroyed on the happy path. `run_snapshot_destroy` returns before `DestroyVm` if `Run`, the stop-reason checks, `TakeSnapshot`, or the capture comparison fails. The aggregate helpers also return on the first failed task at `crates/dh-worker/tests/m6_full_api_uds.rs:447`, `crates/dh-worker/tests/m6_full_api_uds.rs:468`, and `crates/dh-worker/tests/m6_full_api_uds.rs:492`, dropping any leases already restored or injected without best-effort cleanup. A partial acceptance failure can therefore leave occupied KVM slots and skip the final `slots_free == 64` check.

Suggested fix: Add lease cleanup ownership around every restored slot. For example, keep a cleanup guard or explicit `destroy_all_best_effort` path that runs for every acquired lease before returning any error, and collect all spawned task results before failing so later slots still get destroyed.
