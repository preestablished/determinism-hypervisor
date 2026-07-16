Branch name: ralph/iteration-110-createvm-image-and-kernel-resolver-seam-for-dh-workerd
Date: 2026-06-15
Reviewer name: Claude Opus

The change adds the dh-workerd CreateVm image resolver seam: MachineConfig BLAKE3 hashes map to a flat local image cache, entries are verified before use, base images are returned as verified FileBase descriptors, and boot blobs are returned as verified bytes with tests covering ELF and bzImage inputs.

Overall verdict: REQUEST_CHANGES

Stats:
- Files changed: 8 tracked files
- Lines changed: 628 insertions, 12 deletions
- Commits reviewed: 0 branch commits; working-tree diff reviewed

