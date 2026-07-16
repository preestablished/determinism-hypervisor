# Suggestions (non-blocking)

## S1 — The decision doc should mention the `blocking::SnapstoreClient` path for the qmp engine

`docs/decisions/snapstore-server-for-tests.md` frames `spawn_store` as the
seam qmp's engine tests build on (lines 62, 67-69), but the doc only ever
talks about the async `SnapstoreClient`. qmp's vCPU work is
blocking/synchronous (KVM ioctls run on a dedicated thread), and the sibling
already ships the bridge for exactly this case:

- `snapstore-client/src/blocking.rs:1-8` — "Blocking facade over the async
  `SnapstoreClient`. Owns a `current_thread` tokio `Runtime` and delegates
  each method via `block_on`. … **Intended for KVM vCPU worker loops that
  are not tokio-native** (sync-async bridge design note, decision d)."

The engine test will almost certainly want `blocking::SnapstoreClient`, not
the async one `spawn_store` hands back. A one-line note in the Consequences
section ("qmp's engine, being a blocking vCPU loop, consumes the store via
`snapstore_client::blocking::SnapstoreClient`; `spawn_store` returns the
async client for the pure-store joint tests, and qmp can connect a blocking
client to the same `uds_path`") would save the qmp author a discovery cycle.
Note `spawn_store` returns `ServerHandle` + the *async* client + `TempDir`
but not the `uds_path` — see S2.

## S2 — `spawn_store` discards `uds_path`; qmp may need it to attach a second (blocking) client

`spawn_store` (store_joint.rs:18-38) consumes `uds_path` to build one async
client and drops it. The hermetic-isolation test only needs the returned
client, so this is fine *today*. But the moment qmp wants a `blocking`
client against the same server (S1), or a second client of either flavor, it
will need the socket path the helper currently swallows. Consider either
returning `uds_path` as a 4th tuple element, or deriving it deterministically
(it is `TempDir.path().join("snapstore.sock")`, set at line 24 — so a caller
*can* reconstruct it from the returned `TempDir`, which is arguably fine and
worth a one-line comment rather than a signature change). Cheapest fix: add a
comment noting `dir.path().join("snapstore.sock")` is the live socket.

## S3 — `ServerHandle::shutdown()` is never called; teardown relies on drop ordering

`ServerHandle::shutdown(self)` (build_server.rs:56-60) sends a oneshot to
stop the serve loops, but the tests bind it as `_handle` and let it drop at
scope end. Drop does *not* trigger shutdown — only `TempDir` drop removes the
directory, and the server tasks are abandoned to be reaped at process exit.
For 3 short tests this leaks nothing observable (the test binary exits), and
it matches how the sibling's own tests behave. Still, a one-line note in
`spawn_store` ("`ServerHandle` is intentionally held as `_handle`; the server
tasks are reaped at process exit — there is no graceful per-test shutdown")
would document the deliberate choice so a future reader doesn't "fix" it into
a use-after-shutdown. If a future suite spawns hundreds of stores in one
process, revisit and call `handle.shutdown()` in a guard.

## S4 — The 20ms settle sleep is a copied magic number; consider a readiness probe later

`store_joint.rs:32-33` sleeps 20ms after `serve_for_tests` before connecting,
mirroring the sibling's pattern exactly (`page_channel_fallback.rs:67`) — so
this is the right thing to do *now* (parity with the proven pattern). The
comment already says "same settle delay the sibling's own tests use," which
is the correct justification. Flagging only so it's on record: if the joint
suite ever flakes on a loaded kvm-intel box, the fix is a connect-retry loop
(the client connect at line 34 could be wrapped), not a bigger sleep. Not
worth changing while the sibling uses the same constant — drift between the
two would be worse than the magic number.

## S5 — Document the arch-gating rationale for the dev-deps

`Cargo.toml:45-48` adds `snapstore-server`/`snapstore-manifest`/`tempfile`/
`tokio` to the root, and the test file is `#![cfg(target_arch =
"x86_64")]` (store_joint.rs:6). The server itself is arch-independent, so the
gate is about the *test*, not the dep. That is the right call — the joint
suite lives next to the KVM-gated determinism tests and there's no reason to
build/run it on the aarch64 lane where the rest of `determinism-tests` is
KVM-cfg'd out — but the *why* isn't written down. The decision doc says the
deps are "x86_64 dev-dependencies" (line 52) as if the gating were inherent;
a half-sentence clarifying "gated to x86_64 to match the rest of the
determinism suite's lane, not because the store is arch-specific" would
prevent a future reader from assuming the server can't build on arm. (The
deps in `tests/determinism/Cargo.toml:28-33` are plain `*.workspace = true`
with no per-target gate, which is correct since the *file* gates itself.)
