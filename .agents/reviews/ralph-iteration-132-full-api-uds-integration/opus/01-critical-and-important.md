# Critical And Important Findings

## Critical

Severity: Critical

File: `crates/dh-worker/tests/m6_full_api_uds.rs:172`

Problem: `acceptance_slot_cores_or_skip` returns `None` for missing `/dev/kvm`, unreadable CPU topology, or unavailable configured slot cores, and the test body then returns normally at `crates/dh-worker/tests/m6_full_api_uds.rs:504`. That means the documented real acceptance invocation with `--ignored` can report a passing test without restoring, injecting, running, snapshotting, destroying, or comparing any of the 64 slots. This is false-positive acceptance behavior.

Suggested fix: Make prerequisite failures fail the ignored acceptance run loudly. Return `Result<Vec<u32>, String>` or panic/assert with a clear message for missing KVM, unreadable CPU information, or unavailable slot cores. If local developers need a non-acceptance skip mode, put it behind an explicit opt-in variable such as `DH_M6_ACCEPT_ALLOW_SKIP=1`; the documented acceptance command must fail when prerequisites are not satisfied.

## Important

Severity: Important

File: `crates/dh-worker/tests/m6_full_api_uds.rs:318`

Problem: Leases that have already been returned by the API are not reliably destroyed on partial failure. `run_snapshot_destroy` returns before `DestroyVm` if `Run`, `TakeSnapshot`, or the `CaptureSpec` byte check fails; `inject_all` and `run_snapshot_destroy_all` return on the first task error and stop awaiting later join handles, making their moved leases inaccessible; and `create_base_snapshot` also skips destroying the base lease if base snapshot creation fails. A failed 64-slot acceptance run can therefore leave live KVM VMs/slot actors until broader runtime teardown rather than proving the API lifecycle cleanup it claims to exercise.

Suggested fix: Track every acquired lease with an explicit cleanup guard or cleanup phase. Always await all spawned per-slot tasks, collect successes and failures, and issue `DestroyVm` for every lease not known to be destroyed before returning the test error. Apply the same guard pattern to the base snapshot helper so the base VM is destroyed if `TakeSnapshot` fails.
