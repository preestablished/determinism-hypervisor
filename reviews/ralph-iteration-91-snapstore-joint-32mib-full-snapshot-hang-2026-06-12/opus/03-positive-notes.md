# Positive Notes

### P1 — The fix is exactly the textbook remedy, minimally applied

`snapshot-store/crates/snapstore-client/src/client.rs:160-163` replaces the
channel-prefill with `inner.put_pages(tokio_stream::iter(messages))`. This is
the precise pattern prescribed by `tokio-channel-streaming-deadlocks.md`:
"When all messages are already materialized in a `Vec`, pass
`tokio_stream::iter(vec)` — no channel, no task, no deadlock surface." The
change removes the entire deadlock surface rather than papering over it (e.g.
bumping the channel capacity, which would just move the cliff). The unused
`ReceiverStream` import was correctly removed in the same commit.

### P2 — The inline comment explains the failure mode, not just the change

`client.rs:155-159` documents *why* the channel deadlocks ("nothing drains the
receiver until `put_pages` is awaited (>16 chunks = >4096 pages hung forever;
bead 0vl)"). This is exactly the kind of non-obvious-logic comment that earns
its place — a future maintainer is unlikely to reintroduce a bounded channel
here after reading it.

### P3 — Retry-safety is preserved by construction

Because `messages` is rebuilt inside the `with_retry` closure from a fresh
`pages.clone()` (`client.rs:128-151`), the new `tokio_stream::iter` gets a fresh
owned stream on every attempt. The author kept the materialization *inside* the
retried closure rather than hoisting it out — which is what makes the iterator
stream safe under retry. Subtle and correct.

### P4 — The hang-class test carries its own watchdog (the right test shape)

Both regression tests wrap the put in a spawned worker thread and gate it behind
`recv_timeout(120s)` (`snapstore_large_put.rs:36-62`,
`test_cases.rs:621-645`). This converts a regression into a *loud failure*
instead of an unbounded suite hang — precisely the discipline
`tokio-channel-streaming-deadlocks.md` calls for ("Tests for hang-class bugs
must carry their own watchdog"). The commit message documents the test was
demonstrated red (timeout) on the unfixed code and green (<1s) on the fix,
satisfying the "show it red first" criterion.

### P5 — The joint test provably hits the fixed path, with the reasoning written down

`snapstore_large_put.rs` drives the real in-process store over UDS with
`page_channel_path: None`, so it cannot silently fall through the page-channel
fast path and skip the fix. The header comment even pre-empts the obvious
question ("No KVM needed — the hang lived entirely in client/store plumbing")
and ties the size choice to the M4 128 MiB acceptance target (bead 9sb), giving
the test a forward-looking justification beyond the immediate regression.

### P6 — Distinct page contents defeat server-side dedup

Both tests fill `data[..8]` with `i.to_le_bytes()`
(`snapstore_large_put.rs:42-44`), guaranteeing 8192 *distinct* hashes so the
server's content-dedup cannot collapse the batch below the deadlocking chunk
count. A naive all-zero-pages test would dedup to one page and never reach the
17th chunk — the author anticipated and defeated this with an explicit comment
("dedup must not shrink what goes over the wire below the deadlocking chunk
count").

### P7 — Strong roundtrip assertions, not just "didn't hang"

Both tests follow the put with a `get_snapshot` roundtrip and assert the decoded
manifest references **every** page (`manifest.entries.len() == PAGES`), and the
sibling test additionally checks `manifest.guest_ram_bytes`
(`test_cases.rs:659`). This verifies the fix actually *uploaded and stored* all
pages, not merely that the call returned — exercising the contract, not just the
absence of a hang (`rust-integration-testing.md`: "Do tests exercise the
contract, not the implementation's internals?").

### P8 — The ring_chaos comment update is honest

`ring_chaos.rs:8-11` no longer claims the 16 MiB slot is *forced* by the hang;
it correctly reframes the size as "plenty for the 3072 dirtied pages that drive
the overflow" and points at `tests/snapstore_large_put.rs` as the pin for the
now-fixed hang. The comment matches reality — the ring-chaos workload genuinely
needs only 3072 pages, so the slot size is now a sufficiency choice, not a
workaround. No stale "this hangs today" language left behind.
