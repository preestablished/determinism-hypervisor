# Review Overview

- **Branch:** `ralph/iteration-19-wire-detguest-host-path-dependency-on`
- **Base:** `main`
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus (2nd reviewer)
- **Bead:** determinism-hypervisor-2w8 (follow-up: determinism-hypervisor-nln promotes the deps to `[dependencies]`)

## Summary

This is a small, purely additive plumbing change that wires the sibling-repo
crates `detguest-host` and `detguest-wire` (`../guest-sdk/crates/...`) into
`[workspace.dependencies]`, mirroring the existing
`determinism-proto -> ../control-plane` path-dependency pattern. Both crates are
added as `[dev-dependencies]` of `crates/dh-devices` (correctly test-only for
now) and a single integration test file, `tests/detguest_host_smoke.rs`,
exercises the Milestone-1 host API surface this repo will consume: `Channel::attach`
(happy + unmapped-GPA error), `drain_events` on empty rings, `push_command`
publishing through `ChannelWriteSink`, `read_manifest` including the bounded
seqlock-livelock path, `read_region` name resolution, and `InjectResponder`
answering via the PIO sink. I read the consumed sibling sources in full
(`channel.rs`, `manifest.rs`, `inject.rs`, wire `header.rs`/`manifest.rs`,
`ports.rs`) and verified every assertion in the smoke test against the real
contract. The test builds with no warnings and all 6 cases pass. The lock-file
diff is exactly the new transitive closure (`detguest-host`, `detguest-wire`)
and nothing else churns. The change does what it claims and the test asserts
real contracts (error variants, sink invariants, the livelock bound) rather than
re-implementing production logic.

## Verdict

**APPROVE**

The only findings are minor (a doc-comment imprecision and a couple of optional
hardening/coverage suggestions). Nothing blocks merge for an additive
dev-dependency + smoke-test change at this milestone.

## Stats

- **Files changed:** 4 (`Cargo.toml`, `Cargo.lock`, `crates/dh-devices/Cargo.toml`, `crates/dh-devices/tests/detguest_host_smoke.rs`)
- **Lines added:** 173
- **Lines removed:** 0
- **Commits:** 1 (`16f1b6d` ralph: iteration 19 checkpoint)
- **Test result:** 6 passed, 0 failed; clean build, no warnings.
