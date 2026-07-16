Branch name: ralph/iteration-110-createvm-image-and-kernel-resolver-seam-for-dh-workerd
Date: 2026-06-15
Reviewer name: Claude Opus (2nd reviewer)

The branch introduces the CreateVm resolver/cache seam for dh-workerd, documenting the flat BLAKE3 cache contract and implementing verified base-image and boot-blob resolution before the lifecycle RPCs are wired to real KVM success.

Overall verdict: REQUEST_CHANGES

Stats:
- Files changed: 8 tracked files
- Lines changed: 628 insertions, 12 deletions
- Commits reviewed: 0 branch commits; working-tree diff reviewed

