# Review: iteration 91 — snapstore joint 32 MiB full-snapshot hang

- **Branch:** `ralph/iteration-91-snapstore-joint-32mib-full-snapshot-hang`
- **Date:** 2026-06-12
- **Reviewer:** Claude Opus (2nd reviewer)
- **Bead:** determinism-hypervisor-0vl

## Summary

This change fixes a capacity-dependent client-streaming deadlock in the sibling
snapshot-store client and pins it with regression tests in both repos. The root
cause was textbook (and exactly the anti-pattern catalogued in
`tokio-channel-streaming-deadlocks.md`): `SnapstoreClient::put_pages` pre-filled a
bounded `mpsc::channel(16)` with every `PutPagesRequest` *before* handing the
receiver to tonic, so the 17th send parked forever on a `current_thread` runtime
with no second worker to drain it. 4096 pages chunk into exactly 16 messages,
which is why 16 MiB slots worked and 32 MiB (8192 pages → 32 chunks) wedged. The
fix is the correct one — the messages are fully materialized, so the channel buys
nothing and the code hands tonic a `tokio_stream::iter(messages)` directly,
deleting the deadlock surface entirely rather than papering over it with a larger
capacity or a spawned producer. The server-side `put_pages` handler is sound
(it spawns the blocking consumer *first*, then feeds a bounded(4) channel with
live backpressure), so it carries no analogous hang to 128 MiB. Both regression
tests carry a watchdog thread + `recv_timeout(120s)` so a regression fails loudly
instead of hanging CI, and both were demonstrated red pre-fix / green post-fix.
The dh-worker joint test correctly exercises the real in-process gRPC server with
`page_channel_path: None`, so the page-channel fast path cannot mask the fix.

## Verdict

**APPROVE**

The fix is correct, minimal, well-reasoned, and matches the documented best
practice. The tests are well-constructed and genuinely guard the contract. The
findings below are non-blocking: one Important-leaning suggestion about asserting
`guest_ram_bytes` in the dh-worker test for parity with the client test, and
suggestions around the surviving per-page double-allocation that the 9sb 128 MiB
target will amplify. None of these block merge.

## Stats (both repos)

| Repo | Files changed | +/− | Commits |
|------|---------------|-----|---------|
| determinism-hypervisor | 2 (`snapstore_large_put.rs` new, `ring_chaos.rs` comment) | +71 / −3 | 1 (`43bd997`) |
| snapshot-store | 2 (`client.rs`, `tests/test_cases.rs`) | +61 / −10 | 1 (`ab953a5`) |
| **Total** | **4** | **+132 / −13** | **2** |
