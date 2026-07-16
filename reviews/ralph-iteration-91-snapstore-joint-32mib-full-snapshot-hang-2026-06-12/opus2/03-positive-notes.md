# Positive Notes

## P1 — Root-cause fix, not a capacity bump

`../snapshot-store/crates/snapstore-client/src/client.rs:160-161`. The obvious
"fix" for a bounded(16)-channel deadlock is to bump the capacity, which would just
move the cliff to the next size. Instead the change deletes the channel entirely
and hands tonic `tokio_stream::iter(messages)`. This is exactly the remediation
`tokio-channel-streaming-deadlocks.md` prescribes ("When all messages are already
materialized in a `Vec`, pass `tokio_stream::iter(vec)` — no channel, no task, no
deadlock surface") and it removes the entire class of bug, not the one instance.
The now-unused `ReceiverStream` import is removed in the same change — no dead code
left behind.

## P2 — Both regression tests carry their own watchdog

`crates/dh-worker/tests/snapstore_large_put.rs:43-65` and the client test in
`../snapshot-store/.../test_cases.rs`. The bug class is a *hang*, and a naive test
would convert a regression into an unbounded CI hang. Both tests run the put on a
spawned thread and gate on `recv_timeout(120s)`, turning any future regression
back into a loud, attributable failure — precisely the discipline the research
file calls out ("Tests for hang-class bugs must carry their own watchdog ...
otherwise a regression converts the test suite into an unbounded hang").

## P3 — Test sizing is exactly at the failure boundary, with a comment explaining why

`snapstore_large_put.rs:24` (`PAGES = 8192`) and the inline rationale at lines
1-12. The test pins 8192 pages = 32 chunks, well past the 16-chunk cliff, and the
header comment spells out *why* 4096 (= exactly 16 chunks) masked the bug. This is
the capacity-dependent-bug pitfall from the research ("a pre-filled bounded(N)
channel works for ≤N items and wedges at N+1, so small-payload tests pass while
production sizes hang") documented directly in the regression that guards it.

## P4 — Distinct page contents defeat server-side dedup

`snapstore_large_put.rs:34-40` and the client test. Each page's first 8 bytes are
its index (`data[..8].copy_from_slice(&i.to_le_bytes())`), with an explicit comment
that "dedup must not shrink what goes over the wire below the deadlocking chunk
count." Identical pages would dedup server-side and could shrink the effective
chunk count — a subtle way a regression test could silently stop exercising the
boundary. Catching this is good defensive test design.

## P5 — The dh-worker test exercises the real gRPC path, not a fast-path shortcut

`snapstore_large_put.rs:31` (`spawn_store_blocking`) → `common/mod.rs:62`
(`page_channel_path: None`). The page-channel fast path in `put_pages`
(`client.rs:106-124`) only activates for `Transport::Auto` with a configured
channel path; the joint test uses a real in-process server over UDS with the page
channel disabled, so it genuinely drives the gRPC client-streaming path the fix
touches. A fast-path bypass would have made the test green regardless of the fix —
this avoids that trap.

## P6 — Server-side handler has no analogous hang to 128 MiB

`../snapshot-store/crates/snapstore-server/src/service.rs:130-217`. Worth recording
as a positive because it's the natural follow-up worry for the 9sb 128 MiB target:
the server spawns the blocking ingest consumer *first* (line 133), then feeds the
bounded `sync_channel(4)` in a loop while that consumer drains concurrently
(lines 173-214). That is the correct producer/consumer ordering — backpressure
without deadlock — and it scales to arbitrary chunk counts. The error paths
(`msg.pages.len() > 256`, wrong page size) drop the sender and `await` the handle
before returning, so they don't leak the blocking task. No fix needed; the bug was
client-side only.

## P7 — Clear, durable commit message and joint-pin rationale

`ab953a5` and `snapstore_large_put.rs:1-16`. The commit message states the
mechanism, the exact boundary (4096 fits, 4097 hangs), the red→green evidence, and
why the channel was redundant. The joint test's header explicitly ties the pin to
bead 9sb. This is the kind of context that survives long after the author is gone.
