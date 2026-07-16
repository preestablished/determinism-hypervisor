## Positive Notes

- `docs/phase-2-exit-gate.md:3` explicitly records the determinism-class host and the current shell's CPU-affinity limitation, which keeps the sign-off from implying that full M7 slot-core acceptances were rerun in this restricted shell.
- `docs/phase-2-exit-gate.md:17` gives clear sibling-repo ownership boundaries for `snapshot-store`, `guest-sdk`, and `control-plane`; this matches the repo's existing path-dependency and proto-seam model.
- `docs/phase-2-exit-gate.md:64` anchors DHSNAP, DHILOG, record/replay corpus, and device snapshot formats to concrete fixture/test paths rather than broad milestone language.
- `docs/phase-2-exit-gate.md:73` correctly frames snapshot and restore perf thresholds as accepted-as-measured regression gates, not the original aspirational plan numbers.
- `docs/phase-2-exit-gate.md:103` separates M7 harness compile/runnability evidence from the full operator-run acceptance commands, which avoids the main process risk for this kind of close-out document.
- `docs/ops/test-partitioning.md:81` adds an operational reminder to keep the Phase-2 exit-gate record synchronized with fresh evidence and cross-repo ownership splits.
