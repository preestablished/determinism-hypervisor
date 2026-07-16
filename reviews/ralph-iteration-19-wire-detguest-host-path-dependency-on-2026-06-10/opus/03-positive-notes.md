# Positive Notes

### P1 — Dev-dependency placement is exactly right

`crates/dh-devices/Cargo.toml:14-16` puts `detguest-host` / `detguest-wire` under
`[dev-dependencies]`, keeping them out of the production dependency graph until
the real consumer (bead `nln`) lands. This is precisely what
`cargo-workspace-path-deps.md` and `rust-integration-testing.md` prescribe
("keep test-only external deps in `[dev-dependencies]` so the production
dependency graph stays clean"). The comment at lines 12-13 documents the
promotion plan, so the temporary placement won't be mistaken for an oversight.

### P2 — Workspace-level declaration with `workspace = true` inheritance

`Cargo.toml:27-28` declares the path deps once under `[workspace.dependencies]`,
and the member inherits with `.workspace = true`. This is the anti-version-drift
pattern from the research note — no re-declared versions, single source of truth.

### P3 — The "why detguest-wire too" comment

`Cargo.toml:23-26` explains the non-obvious coupling: `detguest-wire` is pulled
in alongside `detguest-host` because `ChannelWriteSink`'s signature exposes
`detguest_wire::RingId`, which `detguest-host` does not re-export. This is exactly
the kind of "docs where logic is non-obvious" comment that saves the next person a
grep. Verified accurate against `detguest-host/src/lib.rs:33-42`.

### P4 — Clean lock-file diff

`Cargo.lock` gains only the two new packages and their single transitive edge
(`detguest-host -> detguest-wire`), plus the two new edges on `dh-devices`.
Nothing else churns — matching `cargo-workspace-path-deps.md`'s review checklist
("the lock file diff matches the manifest change, nothing else churns").

### P5 — `assert!(matches!(...))` for enum-shape assertions

The test uses `assert!(matches!(err, AttachError::Mem(_)))`,
`matches!(..., Err(WireError::SeqlockLivelock))`, and a guarded
`SinkOp::RingPush { .. } if *new_prod > 0 && !bytes.is_empty()` at
`tests/detguest_host_smoke.rs:53, 82-86, 116-118, 125-128`. This is the ergonomic,
refactor-resilient enum-shape style recommended by `rust-integration-testing.md`,
and the guard on `RingPush` asserts the *invariant* (published index advanced,
bytes non-empty) rather than over-fitting to an exact byte layout.

### P6 — The seqlock-livelock boundary is the right thing to pin

`tests/detguest_host_smoke.rs:90-119` deliberately wedges the generation odd and
asserts the bounded retry surfaces `SeqlockLivelock` "instead of a hang." Pinning
the *deterministic, reported* failure of a lock-free read protocol is exactly the
review priority called out in `spsc-ring-memory-ordering.md`. The comment on lines
100-101 explains the scenario (writer mid-update forever) so the magic
`generation: 1` isn't mysterious.

### P7 — Honest, well-scoped module doc

The file header (`tests/detguest_host_smoke.rs:1-8`) states plainly that this is a
"linkage + contract check over `MockGuestMem`, not a detchannel implementation,"
names both beads, and points at where the real implementation lands. Sets correct
expectations for a reviewer and prevents scope-creep complaints.
