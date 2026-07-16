## Action Items

### Critical

None.

### Important

- [ ] `docs/phase-2-exit-gate.md:44` Rewrite the fork note so explicit entropy seeds are documented as optional API behavior: absent/nonzero seed handling should match `fork_engine.rs`, while M7 can still be called out as supplying explicit per-child seeds.

### Suggestions

- [ ] `docs/phase-2-exit-gate.md:73` Add a direct link from the perf section to `docs/upstream-divergences.md` ledger #20 so the accepted-as-measured thresholds are easy to audit.
- [ ] `docs/phase-2-exit-gate.md:98` Add the checkpoint commit or eventual CI run URL to the workspace test/build evidence rows for a stronger sign-off trail.
- [ ] `docs/ops/test-partitioning.md:21` Rename the Phase-2 table row to make clear it is close-out evidence, not a runnable command.
