# Decision: R12 joint tests spawn the real snapstore-server in-process

**Bead:** determinism-hypervisor-wbq · **Status:** decided 2026-06-11 ·
**Owner mechanism:** `tests/determinism/tests/store_joint.rs` (the
`spawn_store` helper)

## Context

Risk R12 forbids mocking the store: M4 acceptance and the snapshot
engine's tests must run against the REAL snapshot-store. Two candidate
mechanisms for getting a server:

1. **Build-and-spawn per test** — hermetic, no provisioning, suspected
   "slower" when framed as spawning a separate binary.
2. **Provisioned long-running service on the kvm-intel box** — fast, but
   an ops surface (systemd unit, upgrade cadence) with HEAD-coupled drift
   risk against the path-dep client, plus shared mutable state across test
   runs (a determinism suite's nightmare).

## Decision

**Build-and-spawn, in-process.** The sibling repo settles the trade
itself: `snapstore-server` exports `serve_for_tests(config) →
(ServerHandle, uds_path)` — a full server (gRPC on UDS, real pagestore,
real durability) started inside the test process on a `TempDir`, the
exact seam snapstore-client's own integration tests use
(`../snapshot-store/crates/snapstore-client/tests/page_channel_fallback.rs`).
The "slower" half of the framing evaporates: there is no second binary
and no rebuild — startup is milliseconds, the store builds once as a
normal workspace dev-dependency.

What this buys:

- **Hermetic**: every test gets a fresh `TempDir` store; no cross-run
  state, no cleanup protocol, parallel tests cannot collide.
- **Zero provisioning**: nothing new on the kvm-intel runner; CI already
  checks out `../snapshot-store` in every lane (iteration 59). Bead py3's
  remaining provisioning list is unaffected.
- **No drift**: server and client come from the same sibling checkout —
  HEAD-wins coupling holds them together by construction.
- **Real durability semantics**: the R12 "refs only after durability"
  acceptance (bead 6hg) is meaningful because the store is the real one
  writing a real pagestore.

Rejected: the provisioned service. Its only advantage (startup latency)
does not exist in the in-process model, and it imports an ops surface +
shared state into a determinism test suite.

## Mechanism

`tests/determinism` adds `snapstore-server`, `snapstore-client`,
`tokio`, `tempfile` as x86_64 dev-dependencies (workspace-managed, same
sibling path-dep pattern as everything else). `store_joint.rs` provides:

```rust
async fn spawn_store() -> (ServerHandle, SnapstoreClient, TempDir)
```

— UDS transport only (no TCP exposure), `TempDir` owns the store's life.
The smoke test in the same file is the R12 ground truth: put_pages →
put_snapshot → get_snapshot byte-identical against the real server. The
snapshot-engine bead (qmp) and M4 ACCEPT (6hg) build on this helper.

## Consequences

- The kvm-intel lane (and any host with the sibling checkout) runs the
  joint tests with zero setup; they are in fact host-runnable — KVM is
  not involved until qmp's engine tests combine `spawn_store` with a
  live slot.
- A future load/perf concern (server per test vs shared) is a test-suite
  refactor, not an ops change — `serve_for_tests_with_metrics` exists if
  shared-registry variants are ever needed.
