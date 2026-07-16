Branch name: ralph/iteration-108-resolve-restoresnapshotresponse-machineconfig-wire-shape
Date: 2026-06-15
Reviewer name: Claude Opus

The change resolves the MachineConfig wire shape needed by RestoreSnapshotResponse by appending cpuid_table and device_set fields to the proto/API contract, pinning prost wire bytes, and adding dh-worker conversion helpers that preserve canonical MachineConfig content while rejecting lossy or invalid wire inputs.

Overall verdict: APPROVE

Stats:
- Files changed: 4 tracked files
- Lines changed: 380 insertions, 1 deletion
- Commits reviewed: 0 branch commits; working-tree diff reviewed

