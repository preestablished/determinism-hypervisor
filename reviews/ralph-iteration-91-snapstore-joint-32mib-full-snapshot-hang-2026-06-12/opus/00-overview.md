# Review Overview — Snapstore 32 MiB FULL Snapshot Hang Fix

- **Branch:** `ralph/iteration-91-snapstore-joint-32mib-full-snapshot-hang`
- **Sibling branch:** `snapshot-store` @ `phase-2-part-1` (commit `ab953a5`)
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus
- **Bead:** determinism-hypervisor-0vl

## Summary

This change fixes a silent, capacity-dependent deadlock in the sibling
snapstore client's `put_pages`. The old code pre-filled a `tokio::sync::mpsc`
bounded channel of capacity 16 with **every** `PutPagesRequest` chunk before
tonic ever polled the receiver, so the 17th `send().await` parked forever —
manifesting as the blocking facade stuck in `ep_poll`, zero CPU, no error.
A 16 MiB slot (4096 pages = exactly 16 chunks of 256) fit the capacity and
masked the bug; a 32 MiB slot (8192 pages = 32 chunks) wedged. The fix
replaces the channel-prefill with `inner.put_pages(tokio_stream::iter(messages))`
— handing tonic a plain iterator stream over the already-materialized `Vec`,
which is exactly the pattern the research file
`tokio-channel-streaming-deadlocks.md` prescribes for fully-materialized data.
Two watchdog-guarded regression tests pin it: a client-level test in the
sibling repo and a joint test in `dh-worker` that drives 8192 pages through
the real in-process store over UDS (verified to exercise the gRPC path, not
the page-channel fast path). The fix is correct, minimal, well-explained, and
the tests are sound. The `ring_chaos.rs` header comment is updated honestly to
reflect that the 16 MiB slot size is now a deliberate sufficiency choice rather
than a hang workaround.

## Verdict

**APPROVE**

## Stats (both repos)

| Repo | Files changed | Lines +/- | Commits |
|------|---------------|-----------|---------|
| determinism-hypervisor | 2 (1 new, 1 comment) | +71 / -3 | 1 (`43bd997`) |
| snapshot-store | 2 (1 src, 1 test) | +61 / -10 | 1 (`ab953a5`) |
| **Total** | **4** | **+132 / -13** | **2** |

- `snapshot-store/crates/snapstore-client/src/client.rs` — channel-prefill → `tokio_stream::iter`; dropped unused `ReceiverStream` import.
- `snapshot-store/crates/snapstore-client/src/tests/test_cases.rs` — new test `blocking_put_snapshot_from_parts_32mib_does_not_hang`.
- `dh-worker/tests/snapstore_large_put.rs` — NEW joint regression `full_32mib_put_snapshot_from_parts_completes`.
- `dh-worker/tests/ring_chaos.rs` — header comment update only.
