# Suggestions (non-blocking)

## 1. Document the in-process boundary: what this proves vs. what needs the chaos bead

**Severity:** Low (documentation). **File:** `crates/dh-worker/tests/store_durability.rs:1-17` (module doc).

The module doc frames the test as proving "the store acked before persisting … was a lie." That is true for the *graceful* restart it performs, but a reader could over-read it as proving crash/power-loss durability. The honest scope is: **bytes are fdatasync'd before ack, and a fresh server process over the same bytes serves them back.** It does *not* exercise torn writes, fsync-lying filesystems, or a kill mid-`put_snapshot`.

Worth one sentence so the next author doesn't think v1n is already covered:

```rust
//! SCOPE: this proves the ack-time durability contract — pages + manifest are
//! fdatasync'd before `take_snapshot` returns (snapstore put_snapshot's
//! group-commit barrier), so a fresh process over the same data_root serves
//! them back. Crash-consistency under a kill mid-write / power loss (torn
//! writes, fsync-lying fs) is the fault-injection bead v1n, which drives the
//! store's `fail_point!`s (manifest-fsync/rename/dirsync, pack-fdatasync).
```

## 2. The graceful shutdown is incidental — make the test say so (and consider making it provable)

**Severity:** Low. **File:** `crates/dh-worker/tests/store_durability.rs:174-178`.

The ordering `drop(store1); handle1.shutdown(); drop(rt1);` reads as if shutdown were load-bearing. It is not: `ServerHandle::shutdown` only `send`s a oneshot and returns immediately (`build_server.rs:56-61`) — it neither waits for in-flight work nor triggers any extra flush, and there is no flush-on-`Drop` in the pagestore/store (only `MetaDbInner` joins its actor thread, and the snapshot/restore path used here never touches meta nodes). So durability is entirely a property of put-time fsync, and **the test would still pass if `shutdown()` were a no-op or if the server were hard-killed.**

That is a *strength* — but the comment "KILL the server … nothing of instance 1 survives but the bytes under data_root" slightly oversells `shutdown()` as the kill. Two cheap options:

- **(a) Cheapest — just sharpen the comment** to note durability does not depend on graceful shutdown (it was already complete at ack), so this is closer to a kill than a clean stop.
- **(b) Strengthen provably** — before `shutdown()`, assert the manifest bytes already exist on disk, e.g. `assert!(data_root.join("store/manifests").read_dir().unwrap().next().is_some())` or scan for the `<hex>.spm` for `delta.snapshot_ref`. That nails "durable *before* any shutdown opportunity" in-process and closes the "maybe shutdown flushed it" reading entirely. Low effort, high signal.

Either is optional; the current test is correct as-is.

## 3. `_rt2`/`_handle2` are dropped at end of scope while instance 2 is mid-use — fine, but note the implicit keep-alive

**Severity:** Low (readability). **File:** `crates/dh-worker/tests/store_durability.rs:181`.

`let (_rt2, _handle2, store2) = ...` binds the runtime and handle to `_`-prefixed locals so they live to end-of-function — correct, because dropping `_rt2` early would tear down the runtime the blocking client bridges into. This mirrors the `rt1/handle1` pattern but the leading underscore can read as "unused/ignorable." Since the binding is load-bearing for lifetime, a one-word comment (`// keep rt2/handle2 alive: store2 bridges into rt2`) would prevent a future cleanup from "tidying" them into `let (_, _, store2)` and breaking the test. The existing `store_joint.rs` has a similar LIFETIME NOTE block; a pointer to it would do.

## 4. Re-snapshot ref-identity assertion is the strongest leg — consider asserting `ref_b2 == delta`-era identity is *not* expected

**Severity:** Low (clarity, optional). **File:** `crates/dh-worker/tests/store_durability.rs:281-302`.

`assert_eq!(ref_b2, ref_b1, ...)` is the right invariant: a FULL re-snapshot of the restored machine is content-addressed, so instance 1 and instance 2 must mint the same 32 bytes. Good. One subtlety a future reader may trip on: `ref_b1`/`ref_b2` are *not* expected to equal `delta.snapshot_ref` (the delta is a parent-relative container; the re-snapshot is FULL). The assertion message "restart changed the bytes a restore reproduces" is apt but slightly cryptic. Consider: `"a FULL re-snapshot of the restored machine is content-addressed — the restarted store must reproduce instance 1's exact ref"`. Purely cosmetic.
