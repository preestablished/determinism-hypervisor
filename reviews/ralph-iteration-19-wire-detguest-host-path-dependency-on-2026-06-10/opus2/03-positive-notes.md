# Positive Notes

## P-1 — Test-only deps correctly scoped to `[dev-dependencies]`

- **File:** `crates/dh-devices/Cargo.toml:14-16`
  ```toml
  [dev-dependencies]
  detguest-host.workspace = true
  detguest-wire.workspace = true
  ```
  The deps are added as dev-dependencies, so they never enter the production
  dependency graph until the real detchannel host side lands (bead nln). This is
  exactly what both the cargo-workspace-path-deps and rust-integration-testing
  research recommend ("Keep test-only external deps in `[dev-dependencies]` so the
  production dependency graph stays clean"). The inline comment even names the bead
  that will promote them.

## P-2 — Shared `[workspace.dependencies]` inheritance, no version re-declaration

- **File:** `Cargo.toml:27-28`, `crates/dh-devices/Cargo.toml:15-16`
  The deps are declared once under `[workspace.dependencies]` and inherited with
  `detguest-host.workspace = true`. This avoids the silent version-drift pitfall the
  research calls out ("Forgetting `workspace = true` in a member: cargo silently
  treats the dep as independent"). It also faithfully mirrors the established
  `determinism-proto` precedent.

## P-3 — Lock-file diff is exactly the new transitive closure, nothing else churns

- **File:** `Cargo.lock` (diff lines 9-31)
  The lock diff adds only `detguest-host` (depending on `detguest-wire`) and
  `detguest-wire`, plus the two new entries under `dh-devices`. No unrelated
  packages move. This is precisely the "check the lock file diff matches the
  manifest change" hygiene the cargo-workspace research asks for.

## P-4 — Excellent inline rationale for why *both* sibling crates are pulled in

- **File:** `Cargo.toml:23-28`
  ```toml
  # detchannel host side (guest-sdk Ms1): sibling-repo path deps, same pattern
  # as determinism-proto -> ../control-plane. detguest-wire is needed alongside
  # detguest-host because the ChannelWriteSink trait signature takes
  # detguest_wire::RingId, which detguest-host does not re-export.
  ```
  This pre-empts the obvious "why is `detguest-wire` here when `detguest-host`
  re-exports most of its surface?" question. I verified the claim:
  `ChannelWriteSink::ring_push` (lib.rs:37) takes `detguest_wire::RingId`, and the
  test does use bare `detguest_wire::*` imports (events, header, manifest, ports,
  RingId), so the second crate is genuinely required, not redundant.

## P-5 — Tests assert real contracts, including the subtle livelock bound

- **File:** `crates/dh-devices/tests/detguest_host_smoke.rs`
  The tests use `assert!(matches!(...))` for enum-shape error assertions (the
  idiom the rust-integration-testing research prefers) and target documented error
  variants — `AttachError::Mem` for an unmapped GPA, `WireError::SeqlockLivelock`
  for a stuck-odd generation, `RegionReadError::NameNotFound` for an unknown
  region. The seqlock case in particular is the kind of edge that is easy to omit:
  it proves the reader **terminates with a typed error instead of hanging** when
  the writer's generation never goes even, which is the spsc/seqlock correctness
  property the spsc-ring-memory-ordering research flags ("Check the full-ring path:
  ... is the failure deterministic and reported?"). I confirmed against
  `manifest.rs:74-106` that the odd-generation check (`g1 % 2 != 0 → continue`)
  precedes header validation, so `generation: 1` correctly drives the 64-retry
  bound to `SeqlockLivelock` without ever reaching `validate()`.

## P-6 — `push_command` test pins the input-log invariant the hypervisor depends on

- **File:** `crates/dh-devices/tests/detguest_host_smoke.rs:66-88`
  The test asserts that one host-side `push_command` surfaces as **exactly one**
  `SinkOp::RingPush` on ring C with a published producer index (`new_prod > 0`,
  non-empty bytes) and bumps `producer_seqs().ring_c` to 1. This directly exercises
  invariant #1 from the sibling crate's own lib doc ("every such write is reported
  through `ChannelWriteSink`") — the property the input log relies on for
  deterministic replay. Asserting the sink trace rather than poking ring memory is
  the right altitude.

## P-7 — `InjectResponder` unmatched-query path asserts the documented Proceed+metric behavior

- **File:** `crates/dh-devices/tests/detguest_host_smoke.rs:131-148`
  The test drives the "no drained `InjectQuery` for this iseq" case and asserts the
  triple contract: returns `0`, bumps `ch.unmatched_injects`, and emits
  `SinkOp::PioAnswer { port: PORT_INJECT, value: 0 }`. I verified `value == 0`
  equals `FaultDecision::Proceed.pack()` (`ports.rs:107`, `:142`), so the magic `0`
  is the real Proceed encoding, not a coincidence. This matches API.md §5's
  "answer Proceed + warning metric" rule (`inject.rs:56-61`).
