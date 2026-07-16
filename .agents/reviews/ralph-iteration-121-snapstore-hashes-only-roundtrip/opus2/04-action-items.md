# Action Items

## Required

None.

## Recommended

None.

## Optional

1. Consider a true ancestor/delta `baseline_ref` case if `dh-snapshot` starts relying on changed-page-only restore planning.
   - Reference: `crates/dh-snapshot/tests/snapstore_readiness.rs:371`
   - Reference: `crates/dh-snapshot/tests/snapstore_readiness.rs:379`
   - The current same-snapshot baseline coverage is sufficient for this follow-up, but it does not exercise the non-empty Mode B path where a delta child is resolved against a full parent.
