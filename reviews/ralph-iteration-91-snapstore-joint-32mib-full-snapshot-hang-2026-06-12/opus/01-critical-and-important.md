# Critical and Important Issues

## Critical

**None.**

The fix is correct and the regression tests are sound. No security, data-loss,
crash, or broken-functionality issues were found.

## Important

**None blocking.**

The two areas the review brief flagged for scrutiny both check out clean:

### Verified — retry interaction with the iterator stream (NOT a bug)

`put_pages` runs its body inside `with_retry(|| { ... })`
(`snapshot-store/crates/snapstore-client/src/client.rs:127-183`). The `messages`
`Vec<PutPagesRequest>` is **rebuilt from scratch on every attempt** — the closure
clones `pages` (`let pages = pages.clone();`, line 128) and re-chunks it into a
fresh `messages` vec (lines 133-151) before constructing
`tokio_stream::iter(messages)` (line 161). So each retry attempt gets its own
freshly-owned iterator stream; there is no consumed-once iterator that would be
empty on a second attempt, and no move/clone conflict. The replacement is
retry-safe. (`with_retry` is `FnMut`, confirmed in `retry.rs:55-58`.)

### Verified — the joint test exercises the FIXED gRPC path, not the fast path (NOT a bug)

`put_pages` has a Linux page-channel fast path
(`client.rs:106-124`) that, when `self.page_channel` is `Some`, returns before
reaching the gRPC `put_pages`/`with_retry` block — which would silently bypass
the fix. The new DH joint test avoids this: `spawn_store_at` builds the server
config with `page_channel_path: None`
(`dh-worker/tests/common/mod.rs:61`) and connects the client via
`Transport::Uds(uds)` (line 73). The page-channel is only wired under
`Transport::Auto { page_channel_path: Some(_) }` (`client.rs:782-788`), so
`self.page_channel` is `None` and the test genuinely drives the fixed gRPC
streaming path. The sibling client test likewise uses
`Transport::Uds` against a `FlakyServer` (`test_cases.rs:634`). Both tests
exercise the real fix.

### Verified — max message size is within tonic's default decode limit (NOT a bug)

Each `PutPagesRequest` carries up to 256 pages × 4096 B = 1 MiB of payload plus
protobuf framing — comfortably under tonic's 4 MiB default
`max_decoding_message_size`. The server does not lower that limit
(`snapstore-server/src/build_server.rs` adds the service with no size override),
and it independently enforces the same 256-pages-per-message ceiling
(`snapstore-server/src/service.rs:184`), so the client's chunk size of 256 is
aligned with the server contract. No oversized-message risk at 8192 (or the
128 MiB / 128-chunk M4 target the test header cites).
