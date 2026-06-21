# Requirement Audit

Before changing code or closing the bead, prove or disprove each requirement
from current state.

## 1. Parent Status Is Stale, Not Still Blocked

Evidence to gather:

```bash
bd show determinism-hypervisor-3l2
bd show determinism-hypervisor-3l2.1
bd show determinism-hypervisor-3l2.2
bd show determinism-hypervisor-3l2.3
bd show determinism-hypervisor-3l2.4
bd show determinism-hypervisor-3l2.5
bd show determinism-hypervisor-3l2.6
bd show determinism-hypervisor-3l2.7
```

Pass condition:

- All explicit blockers in `3l2` are addressed by closed child beads or current
  code.
- No note names an unresolved external dependency.
- If a child close reason was overly broad, the current tests/code still prove
  the stated behavior.

Fail condition:

- Any parent acceptance requirement lacks code/test evidence.
- Any prerequisite is closed but its claimed behavior is absent or regressed.

## 2. Bisection Uses Recorded Checkpoint Evidence

Inspect:

```bash
rg -n "BISECTION_CHECKPOINT|BisectionCheckpoint|bisection_checkpoint" \
  crates/dh-inputlog/src crates/dh-worker/src crates/dh-worker/tests
```

Pass condition:

- Recorder emits `BISECTION_CHECKPOINT` AUX records only at replayable,
  evidence-backed boundaries.
- VerifyReplay indexes checkpoint records sequence-aware, not by `icount`
  alone.
- VerifyReplay validates snapshot refs before using them for refined
  diagnostics.
- Replay probe capture uses the non-mutating checkpoint snapshot primitive and
  does not append new DHILOG records.

## 3. Refined Divergence Fields Are Evidence-Backed

Inspect:

```bash
rg -n "rip_expected|rip_actual|reg_diff|diff_page_idx|BisectionDivergence" \
  crates/dh-worker/src crates/dh-worker/tests tools/dh-cli/src tools/dh-cli/tests
```

Pass condition:

- `rip_expected` and `rip_actual` come from compared expected/actual snapshots.
- `reg_diff` is postcard-encoded `Vec<RegDiff>` from snapshot comparison.
- `diff_page_idx` comes from page hash comparison and is bounded to the
  supported limit.
- For epoch-hash divergence, `icount_lo == selected.coverage_icount_lo` and
  `icount_hi == selected.coverage_icount_hi`; for terminal divergence,
  `icount_lo == selected.coverage_icount_lo` and `icount_hi == end_icount`.
  The public range must not be fabricated, inverted, outside the evidence
  coverage, or a hard-coded 1024-instruction window.
- `suspected_cause` includes provenance that distinguishes replay-vs-recorded
  checkpoint evidence from coarse fallback evidence.

## 4. Coarse Evidence Does Not Masquerade As Bisection

Inspect:

```bash
rg -n "bisect_on_divergence|checkpoint evidence|coarse:" crates/dh-worker/src/service.rs
```

Pass condition:

- `bisect_on_divergence=true` is the default public behavior.
- A coarse `VerifyProgress::Divergence` maps to
  `FAILED_PRECONDITION` when bisection was requested.
- `bisect_on_divergence=false` returns the old coarse verdict with empty
  evidence-only fields.
- Checkpoint-less or invalid-checkpoint logs do not fabricate an old
  1024-instruction or single-point refined range.

## 5. Service And CLI Surfaces Are Covered

Run focused tests before full validation:

```bash
cargo test -p dh-worker verify_replay_divergence_mapping_is_honest_about_bisection
cargo test -p dh-worker rip_mismatch_produces_postcard_reg_diff
cargo test -p dh-worker page_hash_mismatch_reports_page_index
cargo test -p dh-worker page_hash_mismatches_are_limited_to_first_64_indices
cargo test -p dh-worker verify_replay_rpc_streams_divergence_for_semantically_bad_log
cargo test -p dh-worker verify_replay_rpc_streams_done_for_bisection_checkpoint_log
cargo test -p dh-worker verify_replay_rpc_streams_bisection_divergence_with_checkpoint_evidence
cargo test -p dh-worker verify_replay_rpc_rejects_invalid_bisection_checkpoint_gap
cargo test -p dh-worker --test replay_engine lapc_verify_replay_bisection_reports_lapic_reg_diff_on_mutation
cargo test -p dh-cli bisect
```

Pass condition:

- `bisect=false` streams a coarse `Divergence` with `icount_lo == icount_hi`,
  `rip_expected == 0`, `rip_actual == 0`, empty `reg_diff`, empty
  `diff_page_idx`, and `suspected_cause` containing the `coarse:` provenance.
- `bisect=true` and default `None` fail closed without checkpoint evidence.
- Checkpointed logs stream a refined `Divergence` populated from snapshot
  comparison.
- Checkpointed matching logs still stream `Done`; checkpoint evidence does not
  break successful VerifyReplay.
- Invalid checkpoint metadata fails publicly before KVM replay.
- CLI renders bisection divergence in JSON and human forms.

## 6. Parent Closeout Is Honest

Only close `3l2` if every section above passes or is fixed and then passes. If
any section remains unproven, update `3l2` with the exact missing evidence and
leave it open or blocked as appropriate.
