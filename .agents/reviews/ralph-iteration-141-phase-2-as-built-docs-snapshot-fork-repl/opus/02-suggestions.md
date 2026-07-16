## Suggestions

### Link the perf decision directly from the perf section

Path: `docs/phase-2-exit-gate.md:73`

The text cites "divergence ledger #20" but only links `docs/upstream-divergences.md` later in the device-snapshots row. A direct link from the perf section would make the accepted-as-measured decision easier to audit.

Suggested snippet:

```md
`crates/dh-worker/tests/perf_gates.rs` and
[`docs/upstream-divergences.md`](upstream-divergences.md) ledger #20:
```

### Tie evidence rows to a commit or run artifact

Path: `docs/phase-2-exit-gate.md:98`

The gate table says `cargo test --workspace` and `cargo build --workspace` passed after the docs update. That is acceptable for a local sign-off record, but future readers would have a stronger audit trail if this named the branch checkpoint commit or a CI run URL once available.

Suggested snippet:

```md
| 1 | Workspace non-ignored suite remains green | `cargo test --workspace` on 2026-06-16 after checkpoint commit `<commit>` |
| 2 | Workspace build remains green | `cargo build --workspace` on 2026-06-16 after checkpoint commit `<commit>` |
```

### Make the docs-only row in the test matrix visibly non-command evidence

Path: `docs/ops/test-partitioning.md:21`

The host-runnable table otherwise lists commands. The Phase-2 row is useful, but it is a document, not a runnable gate. Labeling it as an evidence record would reduce the chance that operators interpret it as a command row.

Suggested snippet:

```md
| Phase-2 close-out evidence | [`docs/phase-2-exit-gate.md`](../phase-2-exit-gate.md) | as-built snapshot/fork/replay notes, frozen-format anchors, measured perf numbers, and ownership split vs sibling repos |
```
