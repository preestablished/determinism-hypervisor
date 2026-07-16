# Suggestions (non-blocking)

## S1 — Per-page double allocation in chunk construction will hurt at 128 MiB (9sb)

- **File:** `../snapshot-store/crates/snapstore-client/src/client.rs:136-151`

This predates the change, but the fix's commit message and the new test both call
out that the same path now ships a 128 MiB guest (bead 9sb). With the deadlock
gone, the next bottleneck on that path is memory/CPU, so it is worth noting while
the code is open. Each page is copied **twice** while building messages:

```rust
chunk_pages.push(Bytes::from(data.clone()));          // copy 1: Vec<u8> -> Bytes
...
messages.push(PutPagesRequest {
    pages: chunk_pages.iter().map(|b| b.to_vec()).collect(),   // copy 2: Bytes -> Vec<u8>
});
```

`PutPagesRequest.pages` is `Vec<Vec<u8>>`, so the intermediate `Bytes` buys
nothing — it is allocated and immediately copied back out. For 128 MiB that is
~256 MiB of transient allocation churn on top of the `pages.clone()` the retry
closure already does at line 128. Since `put_snapshot_from_parts` passes
`pages.clone()` into `put_pages` and then *also* keeps `pages` to build the
container (`client.rs:753`+), the original `Vec<u8>` is still owned by the caller
and can simply be referenced:

```rust
for (_, data) in &pages {
    local_hasher.update(blake3::hash(data).as_bytes());
    chunk.push(data.clone());                 // single copy into the message
    if chunk.len() == 256 {
        messages.push(PutPagesRequest { pages: std::mem::take(&mut chunk) });
    }
}
if !chunk.is_empty() {
    messages.push(PutPagesRequest { pages: chunk });
}
```

This drops the `Bytes` round-trip and the unused `bytes::Bytes` import in this
function. Non-blocking; flag for the 9sb perf pass rather than this iteration.

## S2 — `tokio_stream::iter` materializes the whole message Vec; document the memory ceiling

- **File:** `../snapshot-store/crates/snapstore-client/src/client.rs:155-161`

The new comment explains *why the channel was removed* but not the property the
replacement now relies on: the entire `Vec<PutPagesRequest>` (≈ one full copy of
guest RAM) is resident before the RPC starts and stays resident until it
completes. That is fine and is the correct trade for a fully-materialized input,
but for the 128 MiB target it is the dominant allocation. A one-line note keeps
the next reader from "optimizing" it back into a channel:

```rust
// All messages are already materialized in `messages` (~guest_ram_bytes
// resident for the RPC's duration). A bounded channel would only re-introduce
// the >16-chunk deadlock; a plain iterator stream is the right shape here.
```

## S3 — Retry re-materializes all messages on every transient attempt

- **File:** `../snapshot-store/crates/snapstore-client/src/client.rs:127-163`

Worth a sentence in the doc comment (not a code change): because message
construction lives *inside* the `with_retry` closure, every retry re-clones
`pages` and rebuilds all chunks and re-hashes — which is correct (a
`tokio_stream::iter` is single-use, so it must be rebuilt per attempt) but means a
flapping server multiplies the 128 MiB allocation cost per attempt. The current
placement is the right one for correctness; just note it so it isn't mistaken for
accidental work that could be hoisted out of the closure (hoisting would break
retry — the iter would be consumed).

## S4 — `ring_chaos.rs` comment edit leaves an awkward mid-sentence wrap

- **File:** `crates/dh-worker/tests/ring_chaos.rs:11-13`

The edit reads cleanly through the new clause but rejoins the old text at a hard
wrap that now reads oddly:

```
//! pinned by tests/snapstore_large_put.rs; bead 0vl). Ring-full exits
//! are host-visible only and
//! harvest-on-full is
//! loss-free by construction (§8.2): ...
```

The `and / harvest-on-full is / loss-free` split predates this change but the edit
touched the adjacent line, so it's cheap to reflow into normal prose while you're
here. Purely cosmetic.

## S5 — Watchdog leaves the put thread detached on timeout

- **File:** `crates/dh-worker/tests/snapstore_large_put.rs:51-65` and
  `../snapshot-store/crates/snapstore-client/src/tests/test_cases.rs` (same shape)

If the put ever does hang again, `recv_timeout` fires and the test panics, but the
spawned worker thread (and, in the dh-worker case, the still-running server on
`_rt`) is left detached rather than joined/aborted. For a regression watchdog this
is the *intended* behavior — you want a loud panic, not a clean join that waits
forever — so this is acceptable as-is. Noting it only so a future reader doesn't
"fix" it into a `handle.join()` that would re-introduce the unbounded hang the
watchdog exists to prevent. No change recommended; consider a one-line comment
that the detach is deliberate.
