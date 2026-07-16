# 01-critical-and-important.md
## Critical
None.

## Important
- Important - [reader.rs](/home/infra-admin/git/preestablished/determinism-hypervisor/crates/dh-inputlog/src/reader.rs:667), [dhilog.rs](/home/infra-admin/git/preestablished/determinism-hypervisor/crates/dh-inputlog/src/dhilog.rs:354): `BISECTION_CHECKPOINT` duplicates the record `icount` inside the payload, and the writer sets it from the record header icount, but the reader only validates payload length, nested version, and nested flags. A hostile or corrupt sealed log can therefore parse with record header `icount = A` and payload `checkpoint_icount = B`. The API semantics say the checkpoint is captured at the record’s `icount`; downstream bisection code may use either field and produce a misleading evidence window.
  Suggested fix: pass the record header `icount` into kind validation for this record and reject mismatches, either with a specific error or `BadPayloadLayout`. Add a negative test that hand-frames a checkpoint record with mismatched payload/header icount and expects rejection.
