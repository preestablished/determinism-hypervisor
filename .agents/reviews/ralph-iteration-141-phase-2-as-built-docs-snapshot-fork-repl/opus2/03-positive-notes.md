# Positive Notes

- `docs/phase-2-exit-gate.md:3` clearly records the determinism-class host tuple and immediately explains the current CPU-affinity limitation.
- `docs/phase-2-exit-gate.md:7` explicitly preserves full M7 slot-core acceptance as operator-run work instead of claiming it was completed from the housekeeping-only shell.
- `docs/phase-2-exit-gate.md:17` gives a clear ownership split across `determinism-hypervisor`, `snapshot-store`, `guest-sdk`, and `control-plane`.
- `docs/phase-2-exit-gate.md:73` correctly frames the snapshot/restore perf thresholds as accepted-as-measured regression gates rather than the original aspirational numbers.
- `docs/phase-2-exit-gate.md:104` correctly states that a local skip-mode cross-slot run is only guard-path coverage and does not replace the isolated slot-core operator run.
- `docs/ops/test-partitioning.md:81` adds the right maintenance hook so future phase close-out updates keep the sign-off record synchronized with fresh command evidence and ownership changes.
- `README.md:97` makes the phase records discoverable without changing the README's existing "More docs" style.
