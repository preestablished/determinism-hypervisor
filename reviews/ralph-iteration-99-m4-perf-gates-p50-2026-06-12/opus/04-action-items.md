# Action Items

Verdict: **APPROVE**. Nothing here blocks the merge or the 8ot escalation. The items below
improve fidelity and close consumer-contract gaps; the only Important one is a one-comment
scope fix.

### Critical

- [ ] None.

### Important

- [ ] **Document that the timed incremental window excludes ring-harvest/reset cost.**
      In `crates/dh-worker/tests/perf_gates.rs` (the 8k-dirty comment around lines 140-148)
      and `crates/dh-worker/benches/perf_gates.rs`, the dirty set is built via host-side
      `dirty.insert()` with an **empty** KVM dirty ring, so `take_snapshot`'s
      `harvest_at_boundary` (`snapshot_engine.rs:133`) drains 0 entries and skips
      `reset_dirty_rings`. A guest-dirtied 8k run would harvest 8192 ring entries + reset
      ioctls inside the same window, and that cost is part of "incremental snapshot ≤ 8k
      dirty pages" per IMPLEMENTATION-PLAN §M4 line 84. Amend the comment that currently
      claims the set is "exactly the engine path the gate times" to state the harvest is
      excluded by construction, that this is acceptable while the gate is storage-bound
      (111.6 ms vs 15 ms — harvest is sub-ms noise), and that it must be revisited if the
      snapshot path ever becomes harvest-bound. Cross-reference 8ot. (Optional higher-fidelity
      follow-up: dirty via a guest loop + boundary harvest, as
      `incremental_snapshot_ships_exactly_the_dirty_pages_and_clears` already does at 3 pages —
      defer until 8ot fixes the storage path.)

### Suggestions

- [ ] **(S1)** Close the skip-equals-pass hole for the nightly gate (1pa): when an
      env var the nightly job sets is present, turn the `!kvm_available()` and
      `cfg!(debug_assertions)` early returns in `tests/perf_gates.rs:78-87` into `panic!`s
      (or emit a `PERF_GATE_RAN` sentinel line and have 1pa fail if it is absent). Keeps
      ad-hoc operator ergonomics; stops a misconfigured nightly run from passing green
      without measuring. Note on 1pa which side implements it.
- [ ] **(S2)** Add a one-line comment to `p50` (`tests/perf_gates.rs:97`) noting it
      deliberately takes the **upper** median (conservative for a gate) so a future reader
      does not "fix" it into an averaging median that loosens the gate.
- [ ] **(S3)** Optional: note (or add) a warm-up iteration in the test gate loops; the
      median of 30 is already robust to a cold sample 0, so a comment suffices. The bench
      already warms up.
- [ ] **(S4)** No action now — store growth is bounded by content-addressed dedup
      (per-sample content is deterministic). Flag for any future change that varies
      per-sample content: it would defeat dedup and grow the tempdir linearly.
- [ ] **(S5)** Optional: if criterion's "unable to complete N samples in the time limit"
      warning is noisy in the nightly log, bump the snapshot group's `measurement_time` from
      2 s to 3 s (`benches/perf_gates.rs`). Functionally fine as-is — criterion extends
      rather than fails.
