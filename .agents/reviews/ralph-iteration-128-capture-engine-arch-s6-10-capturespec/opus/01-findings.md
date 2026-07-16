# Findings

## P1 - VerifyReplay is not updated for DetChannel-producing recordings

`Run` now services DetChannel PIO exits through `service_exit_with_detchannel`, which can attach the channel, drain rings, and log canonical DetChannel records (`crates/dh-worker/src/service.rs:1326`, `crates/dh-worker/src/service.rs:2402`). However replay still drives guest exits through the generic `DeviceRail::service_exit` (`crates/dh-worker/src/replay_engine.rs:194`-`198`), whose implementation handles serial PIO and MMIO only, not the DetChannel PIO window (`crates/dh-vmm/src/recording.rs:104`-`133`).

That means a log recorded from the new capture fixture or any guest-sdk VM using DetChannel can hit the same DetChannel PIO exit during replay and fail before applying the corresponding log record. The only VerifyReplay RPC coverage in this checkpoint still uses `landing_loop_elf()` without DetChannel (`crates/dh-worker/src/service.rs:3757`-`3855`), so this regression is not exercised.

Recommended fix: make replay use the same DetChannel-aware exit semantics as recording, with careful handling for exit-generated canonical records such as PIO answers and ring consumer bumps so replay does not double-apply them.

## P2 - `Run` commits the new boundary before returning failed-capture errors

On successful execution, `Run` updates runtime position, marks the slot paused, publishes the new manager position, and only then evaluates `capture_at_boundary` (`crates/dh-worker/src/service.rs:2458`-`2490`). If capture fails, for example a `layout_version` mismatch returning `FAILED_PRECONDITION`, the RPC returns an error after the guest has advanced and after the slot manager now reports the new boundary.

This is a behavioral trap for the C2 "invalid step" path: callers receive a failed `Run` without the `RunResponse` boundary/hash, but the lease now points at the post-run VM and the original capture point cannot be retried as the same RPC. `TakeSnapshot` avoids the analogous issue by running capture before publishing the snapshot (`crates/dh-worker/src/service.rs:2602`-`2635`); `Run` should either document and test "run may advance on capture failure" explicitly, or change the API behavior so capture failures are surfaced without hiding the successful run boundary.

Recommended fix: add a `Run` layout-mismatch regression that asserts the intended post-error slot state/position. If the intended contract is atomic failure, the implementation needs a different ordering or a response shape that can report run success plus capture failure.
