# Action Items

Synthesized from `01-critical-and-important.md` and `02-suggestions.md`.
Each item is self-contained for a fixer.

## Action Items

### Critical
- [ ] None.

### Important
- [ ] None blocking. The two near-important items below are tracked as Low and
      do not gate this merge:
  - [ ] [Cargo.toml:27-28] (I-1) Confirm CI checks out `../guest-sdk` at a
        compatible revision when running `cargo test`, the same way it must
        already check out `../control-plane` for `determinism-proto`. If CI does
        not clone siblings generically, file/extend a bead to add the guest-sdk
        checkout step. No code change if CI already handles it.
  - [ ] [crates/dh-devices/tests/detguest_host_smoke.rs:121-129] (I-2) Defer
        `read_region` success + `RegionReadError::OutOfBounds` coverage to bead
        nln (where the real detchannel host side lands); not needed in this
        smoke test.

### Suggestions
- [ ] [crates/dh-devices/tests/detguest_host_smoke.rs:95-97] (S-1) Fix the
      doc comment: the 64 zeroed entries are live-but-nameless, not "dead"
      (`flags == 0`, DEAD is bit 31). Reword to "live-but-nameless, so nothing
      resolves by name." Assertion itself is correct.
- [ ] [crates/dh-devices/tests/detguest_host_smoke.rs:91-119] (S-2) Optionally
      drive the odd generation in the livelock case through the public
      `writer_begin` seqlock helper instead of hand-writing `generation: 1`, for
      symmetry with the agent's real writer protocol and to avoid coupling to the
      generation byte encoding.
- [ ] [crates/dh-devices/tests/detguest_host_smoke.rs:91-119] (S-3) Optionally
      add the seqlock *recovery* leg (writer finishes → even generation →
      `read_manifest()` is `Ok`) so the smoke test self-contained-ly proves the
      retry loop both bounds and succeeds.
- [ ] [Cargo.toml:23-28] (S-4) Add a note to bead nln that promoting the deps
      from `[dev-dependencies]` to `[dependencies]` is a manifest-only move with
      no `Cargo.lock` churn (the transitive closure is identical), so the
      promotion diff can be verified cleanly. No code change.
