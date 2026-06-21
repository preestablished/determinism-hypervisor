# Security And Correctness Review

Reviewer: Bernoulli

## Findings

1. `03-implementation-sequence.md` required reading up to
   `MAX_BASE_IMAGE_BYTES` into a `Vec`, but did not require fallible allocation.
   A 512 MiB under-cap base image can still make unchecked `Vec` allocation
   abort the worker under memory pressure instead of failing closed with an
   actionable CreateVm status. The plan now requires `try_reserve_exact` or
   equivalent fallible growth, an allocation resolver error, and
   `resource_exhausted` service mapping.

2. `04-tests-and-reference-host-validation.md` only proved post-verification
   mutation through direct `FileBase::read_at`. The bead requires safety before
   and during pv-blk reads. The plan now requires a `PvBlk` device-level
   mutation test covering both direct read and write-triggered
   read-modify-write after cache mutation/truncation/growth.

3. The sparse-file test asserted `TooLarge`, but did not prove the
   implementation failed before hashing. The plan now requires the helper to
   check metadata length before allocation/read/hash and adds a small-limit
   helper-level test that asserts `TooLarge.len` is the metadata length.

4. Service mapping coverage was optional and could have been satisfied by
   kernel/initramfs cases. The plan now requires explicit base-image
   `TooLarge`, `HashMismatch`, `NotFile`, and allocation failure service
   mapping tests.

## Assessment

The bounded owned-byte `FileBase` design closes the post-verification mutation
hole if the resolver hashes the exact owned bytes that pv-blk later reads. The
512 MiB cap closes the huge/sparse base-image risk only if the metadata cap is
checked before allocation, reads, and hashing.

## Non-Blocking Suggestions Applied

- The plan now states that memory cost is per runtime and not a global quota.
- The promised review files now exist.
