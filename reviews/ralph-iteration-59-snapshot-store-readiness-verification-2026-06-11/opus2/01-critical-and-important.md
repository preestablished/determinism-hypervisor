# Critical & Important Findings

## Critical

**None.** No security, data-loss, crash, or broken-functionality issues. The change adds a
build-time dependency and a compile-time pin test; it introduces no runtime code path. The
self-hosted runner fork-PR guard is unchanged and remains correct (see positive notes). The
lock-file churn matches the manifest change. `cargo metadata` and the readiness test both
pass locally.

## Important

### I1 — The surface-pin's safety claim is broader than what it actually pins (signatures mostly unchecked)

- **Severity:** Important (correctness of the guarantee, not of the code)
- **File:** `crates/dh-snapshot/tests/snapstore_readiness.rs:16-44`
- **Description:** The module doc and the `_surface_pins` comment frame this test as the
  thing that "breaks *here*, with a readable name, instead of deep inside the M4 snapshot
  engine." But `let _ = SnapstoreClient::put_snapshot;` (and all the other bare
  function-item references) only pins that a method *named* `put_snapshot` exists and is
  callable as a path — it does **not** pin its argument or return types. snapshot-store
  could change `put_snapshot(container: Vec<u8>)` to `put_snapshot(container: Bytes)`, or
  change `get_snapshot`'s return from `Vec<u8>` to a streaming type, and this gate stays
  green while the real M4 engine breaks exactly where the doc promises it won't. Only
  `put_pages` gets a real signature pin (`_put_pages_signature`). The risk here is a
  *false sense of security*: a future maintainer reads the doc, trusts the gate, and
  doesn't add their own contract test when they build the M4 engine.
- **Why it matters long-term:** Bare-fn-item pins are the weakest form of API pin. They
  catch renames/removals/visibility changes but silently tolerate signature drift — which
  is the more common and more insidious breakage for a gRPC client surface that is still
  evolving (the sibling's `Transport::Auto.page_channel_path` is explicitly "reserved for
  WI3," i.e. this surface is *not* frozen).
- **Suggested fix:** Either (a) tighten the doc so it doesn't over-promise — state plainly
  that bare references pin existence/visibility and that signatures are pinned only for
  `put_pages` (and add the others as the M4 engine commits to them); or (b) add signature
  pins for the legs the engine will actually depend on first. A cheap pattern that pins the
  full signature without naming sibling types in this crate is to coerce the method to a
  typed fn pointer:

  ```rust
  // Pins put_snapshot's exact signature: (&self, Vec<u8>) -> impl Future<Output=Result<SnapshotRef, ClientError>>
  // Use the returned-future form like _put_pages_signature to avoid naming SnapshotRef here:
  fn _put_snapshot_signature(
      client: &SnapstoreClient,
      container: Vec<u8>,
  ) -> impl std::future::Future<Output = Result<snapstore_client::SnapshotRef, ClientError>> + '_ {
      client.put_snapshot(container)
  }
  ```

  Note `SnapshotRef`/`LogId`/`PageHash` are re-exported by `snapstore-types`, not by
  `snapstore-client` (the client `use`s them from `snapstore_types`). So a signature pin
  for `put_snapshot`/`get_snapshot`/`resolve_pages`/`put_input_log` needs `snapstore-types`
  as a second dev-dep, *or* should use the return-position-`impl-Trait` trick that lets the
  caller avoid spelling the type. If adding `snapstore-types` is unwanted churn, prefer
  option (a) (fix the doc) for now and file a follow-up bead to add real signature pins
  when the M4 engine lands. (Research ref: rust-by-example dev-dependencies; "Do tests
  exercise the *contract* … not the implementation's internals?" — here the contract is the
  signature, and most signatures are unpinned.)

### I2 — Blocking facade pins omit the input-log legs, but the async side pins them

- **Severity:** Important (coverage asymmetry that will silently regress)
- **File:** `crates/dh-snapshot/tests/snapstore_readiness.rs:38-43`
- **Description:** The async `_surface_pins` block pins `put_input_log` / `get_input_log`
  (lines 35-36) with the comment "DHILOG containers ride the same store." The blocking
  facade block (lines 39-43) pins only `connect`, `put_pages`, `put_snapshot`,
  `get_snapshot`, `resolve_pages` — it does **not** pin `blocking::put_input_log` /
  `blocking::get_input_log`, even though the sibling's `blocking.rs:74-80` exposes both. The
  module doc says the blocking facade exists "for non-tokio KVM vCPU worker loops," and the
  worker loop is precisely where input-log replay would run synchronously. If snapshot-store
  drops the blocking input-log methods, this gate stays green while the async ones still
  pin — an inconsistent guarantee that's easy to miss.
- **Suggested fix:** Add the two missing blocking pins for parity:

  ```rust
  let _ = blocking::SnapstoreClient::put_input_log;
  let _ = blocking::SnapstoreClient::get_input_log;
  ```

### I3 — No clean-state / sibling-absent failure mode is documented or guarded

- **Severity:** Important (operational, low likelihood but high confusion-cost)
- **File:** `Cargo.toml:44`, `.github/workflows/ci.yaml:54-57`
- **Description:** With three sibling path deps now required at `cargo-metadata` time, *any*
  cargo invocation in this repo (even `cargo fmt`, `cargo metadata`, editor rust-analyzer)
  hard-fails if `../snapshot-store` is missing — the failure is a cargo manifest-resolution
  error, not a friendly message. This was already true for two siblings; adding a third
  widens the blast radius and the research notes flag exactly this ("Path deps that work
  locally but break CI … Verify a clean-state build"). The CI checkout ordering is correct
  (sibling checkout steps precede the toolchain/build steps), so CI is fine. The gap is
  developer-facing: a contributor who clones only this repo gets an opaque failure. Nothing
  in this diff points them at the three required siblings.
- **Why it matters long-term:** Onboarding friction and "works on my machine" reports. The
  `cargo fmt --check` step (ci.yaml:71-76) even runs `cargo metadata` first, so a missing
  sibling fails *that* step with a message about metadata, not about the sibling.
- **Suggested fix:** Non-blocking, but worth a one-line pointer in the repo README or
  CONTRIBUTING that lists the three required sibling checkouts and their expected relative
  paths (`../control-plane`, `../guest-sdk`, `../snapshot-store`). Optionally a `cargo
  xtask`/Makefile preflight that checks the three paths exist and prints a clear message.
  This is a documentation follow-up, not a merge blocker.
