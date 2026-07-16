Branch name: ralph/iteration-108-resolve-restoresnapshotresponse-machineconfig-wire-shape
Date: 2026-06-15
Reviewer name: Claude Opus (2nd reviewer)

The change fills the missing RestoreSnapshotResponse MachineConfig shape by adding the canonical cpuid_table and device_set vectors to the proto message and API docs, then mapping those fields to and from dh-vmm::config::MachineConfig with explicit validation and regression tests.

Overall verdict: APPROVE

Stats:
- Files changed: 4 tracked files
- Lines changed: 380 insertions, 1 deletion
- Commits reviewed: 0 branch commits; working-tree diff reviewed

