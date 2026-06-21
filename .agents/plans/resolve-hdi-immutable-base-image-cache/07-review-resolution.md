# Review Resolution

Two subagents reviewed the plan before implementation.

## Accepted Findings

Both reviewers agreed that the bounded owned-byte `FileBase` design is the
right fix for `determinism-hypervisor-hdi`, but found missing details that must
be part of the implementation handoff.

Accepted changes:

- Require fallible allocation while copying base images. The implementation
  must use `try_reserve_exact` or an equivalent fallible growth pattern and map
  memory pressure to an actionable resolver/service error.
- Strengthen mutation tests to exercise the real `PvBlk` device path, including
  a read after cache mutation and a write-triggered read-modify-write after
  cache mutation.
- Pin sparse-file fast-fail behavior at the helper level so oversized metadata
  is rejected before allocation, read, or hashing.
- Require base-image-specific service error mapping tests for `TooLarge`,
  `HashMismatch`, `NotFile`, and allocation failure.
- Clarify that `open_base_image` should return
  `Ok((path, FileBase::from_owned_bytes(out)))` or the equivalent, not pass the
  path into the `FileBase` constructor.
- Document that the 512 MiB cap is per runtime, not a global worker image-memory
  quota.
- Add the review files promised by `00-summary.md`.

## Rejected Or Deferred

No reviewer asked to change the core contract to fs-verity, cache permissions,
or sealed memfd. Those remain rejected for this bead and can be revisited only
as a follow-up if future large-image workloads need an immutable streaming
backend.
