# Suggestions

### Suggestion: make fresh command evidence more auditable

Path: `docs/phase-2-exit-gate.md:98`

Rationale: Phase 1 included result snippets and a CI run identifier. Phase 2 currently lists commands and dates, but not pass snippets, commit SHA, or a log/run anchor. That is acceptable for a local docs update, but this record is meant to be read later as exit-gate evidence.

Snippet:

```markdown
| 1 | Workspace non-ignored suite remains green | `cargo test --workspace`: PASS on 2026-06-16 at commit `089d9eb`, after the Phase-2 docs update |
| 2 | Workspace build remains green | `cargo build --workspace`: PASS on 2026-06-16 at commit `089d9eb`, after the Phase-2 docs update |
```

### Suggestion: label perf numbers as accepted baselines, not fresh measurements

Path: `docs/phase-2-exit-gate.md:73`

Rationale: The perf table is sourced accurately to `perf_gates.rs` and divergence ledger #20, but a reader could conflate these with measurements re-run on 2026-06-16. Naming the 2026-06-12 accepted baseline keeps freshness clear.

Snippet:

```markdown
The measured p50s below are the 2026-06-12 accepted baselines from divergence ledger #20, not a fresh perf re-run in this docs-only sign-off.
```

### Suggestion: mark the Phase-2 row in test partitioning as a reference, not a runnable gate

Path: `docs/ops/test-partitioning.md:21`

Rationale: The surrounding table entries are mostly commands. The Phase-2 exit-gate record is useful there, but it is a document to keep synchronized, not a test someone can run.

Snippet:

```markdown
| Phase-2 exit-gate reference | [`docs/phase-2-exit-gate.md`](../phase-2-exit-gate.md) | reference record, not a command: as-built snapshot/fork/replay notes, frozen-format anchors, measured perf numbers, and ownership split vs sibling repos |
```
