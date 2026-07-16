# Suggestions (non-blocking)

### S-1 — Document the async-only constraint of `spawn_store` (seam 3)

**File:** `tests/determinism/tests/store_joint.rs:18`, `docs/decisions/snapstore-server-for-tests.md`

`serve_for_tests` is `async fn` and spawns server tasks onto the *current* tokio
runtime, so `spawn_store` is inherently `async` and only callable from a
`#[tokio::test]` context. But the KVM engine is **synchronous** — its real
callers use `snapstore_client::blocking::SnapstoreClient`, which owns its *own*
`current_thread` runtime and `block_on`s each call (`blocking.rs:25-39`).

This is a real seam the qmp engine tests (which combine `spawn_store` with a
live KVM slot) will hit: you cannot drive a synchronous vCPU loop and host an
in-process tokio server from the same thread without care. The decision doc says
the joint tests "are in fact host-runnable" but does not call out that the server
side forces an async harness even when the client-under-test is the blocking
facade. A one-paragraph note in the decision doc ("the server half requires a
tokio runtime; sync callers must spawn it on a dedicated runtime/thread and reach
it via `blocking::SnapstoreClient::connect(Transport::Uds(path))`") would save the
qmp author a debugging cycle. The sibling's `blocking_facade_smoke`
(`test_cases.rs:579`) already demonstrates the pattern: server on a separate
multi-thread runtime, blocking client on the test thread.

### S-2 — Add a `ServerHandle`-drop-mid-call hazard line (seam 3)

**File:** `docs/decisions/snapstore-server-for-tests.md:38-43` / `store_joint.rs:18`

`spawn_store` returns `(ServerHandle, SnapstoreClient, TempDir)` and the tests
bind the handle as `_handle` (held to end of scope). Dropping `ServerHandle`
initiates graceful shutdown (build_server.rs:283-308 wires drop → shutdown
broadcast). For a *blocking* caller this is a genuine footgun: if a future helper
returns the client but lets the handle drop, the next blocking call deadlocks or
errors against a shut-down UDS. Worth one doc line: "the `ServerHandle` (and
`TempDir`) must outlive every client call; never let them drop mid-test." The
current tests get this right by binding `_handle`/`_dir`, but the contract is
implicit.

### S-3 — Consider a `blocking`-flavored helper alongside the async one (seam 3)

Since the production consumer is synchronous, a sibling helper
`spawn_store_blocking() -> (ServerHandle, blocking::SnapstoreClient, TempDir, Runtime)`
(server on a held multi-thread runtime, blocking client over the same UDS) would
let qmp's sync engine tests exercise the *exact* facade production uses, rather
than the async client. Not required for this iteration's smoke tests, but the
decision doc's "qmp builds on this helper" promise is cleaner if the helper
matches the production transport facade. File as follow-up under qmp.

### S-4 — Strengthen the fresh-first-put assertion if the comment is corrected

**File:** `store_joint.rs:84-85`

Per finding I-1, `assert_eq!((new, deduped), (3, 0))` is stable on this seam
(246/246). If the team keeps the sum-only form for conservatism that is fine, but
the stricter form would catch a real regression class (a fresh store that
unexpectedly reports dedup — e.g. a pagestore that leaks state across TempDirs,
which is exactly the hermeticity property R12 cares about). The
`re_put_is_deduped_and_ref_stable` test already asserts the strict `(0, 2)` shape
on a *re*-put, so the codebase is comfortable asserting exact splits.

### S-5 — Reduce the per-test fixed 20 ms settle sleep

**File:** `store_joint.rs:32-33`

Each `spawn_store` sleeps a fixed 20 ms "same settle delay the sibling's own
tests use." With three tests plus any qmp/6hg tests sharing the helper this is
pure wall-clock tax, and a fixed sleep is the classic source of *future* flakes
under heavy CI load (too short) while being wasteful on a quiet box (too long).
A readiness probe — poll the health reporter (the server sets the service to
`NotServing`→serving, build_server.rs:260-266) or retry the first
`SnapstoreClient::connect` a few times — would be both faster and more robust.
Low priority; the current sleep is not causing flakes today (0/5 stress runs).
