# Code Review — Overview

- **Branch:** `ralph/iteration-19-wire-detguest-host-path-dependency-on`
- **Base:** `main`
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus
- **Bead:** determinism-hypervisor-2w8 (follow-up impl bead: determinism-hypervisor-nln)

## Summary

This branch wires two sibling-repo path dependencies — `detguest-host` and
`detguest-wire`, both at `../guest-sdk/crates/` — into the workspace, mirroring
the existing `determinism-proto -> ../control-plane` pattern. They are declared
once under `[workspace.dependencies]` and inherited as **dev-dependencies** of
`crates/dh-devices`. A single integration smoke test
(`crates/dh-devices/tests/detguest_host_smoke.rs`, 6 tests) exercises the
guest-sdk Milestone-1 host API surface this repo will consume: `Channel::attach`
(success + unmapped-GPA error), `drain_events` on empty rings, `push_command`
through a `RecordingSink`, `read_manifest` (including the seqlock-livelock bound),
`read_region` name-not-found, and `InjectResponder::answer` via the PIO sink. The
change is purely additive — no production code paths change, and the deps stay out
of the production dependency graph until bead `nln` promotes them.

I verified the branch builds and all 6 tests pass with zero compiler warnings
(`cargo test -p dh-devices --test detguest_host_smoke`). I cross-checked every
asserted contract against the sibling-repo source
(`detguest-host/src/{lib,channel,manifest,inject}.rs`,
`detguest-wire/src/{lib,header,manifest,ports,events}.rs`): the imports resolve,
the error variants asserted (`AttachError::Mem`, `WireError::SeqlockLivelock`,
`RegionReadError::NameNotFound`) are the documented ones, the seqlock bound
(`SEQLOCK_RETRIES = 64`) is real, and `ManifestHeader::write_to` accepts the
test's 32-byte buffer exactly (`OFF_ENTRIES = 0x20`).

## Verdict

**APPROVE** — small, additive, well-documented, green tests, contracts verified
against the consumed API. The findings below are all non-blocking; the only
"important" item is a coverage gap (success path of `read_region`/`drain_events`
with real data is left untested), which is acceptable for a linkage smoke test
and is properly bounded by the follow-up bead.

## Stats

| Metric | Value |
|---|---|
| Commits (`main..HEAD`) | 1 (`16f1b6d`) |
| Files changed | 4 |
| Lines added | 173 |
| Lines removed | 0 |

Files:
- `Cargo.lock` — +13 (transitive closure: `detguest-host`, `detguest-wire`)
- `Cargo.toml` — +6 (2 workspace deps + 4 comment lines)
- `crates/dh-devices/Cargo.toml` — +6 (dev-deps + comment)
- `crates/dh-devices/tests/detguest_host_smoke.rs` — +148 (new file)
