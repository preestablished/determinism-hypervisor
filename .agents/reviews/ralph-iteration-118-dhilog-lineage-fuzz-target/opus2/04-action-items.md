# Action Items

Changes needed before landing:

1. Preserve the 24h operator dispatch semantics in `.github/workflows/nightly-drift.yaml`, or explicitly update the workflow docs to say the self-hosted accept run is now about 48h.
2. Add a deterministic multi-segment splice seed or generation path so `dhilog_splice` reliably reaches successful `Lineage::new/extend/edges` with `len > 1` in CI.

No production code edits are recommended beyond the fuzz/workflow/test-support surface already in scope for bead `determinism-hypervisor-6zm`.
