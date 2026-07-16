# Positive Notes

- The branch correctly identifies the dangerous part of the previous behavior: `live_end = header.end_state_hash` was gated only on `terminal_sdk_target.is_some()`, so any terminal SDK target could mask an exact final-state mismatch.
- The new helper makes the substitution policy explicit and locally testable.
- The exact `BudgetReached`, exact `GuestHalted`, non-`BudgetReached`, before-event, and no-tail negative cases in `terminal_sdk_end_hash_substitution_is_limited_to_early_hlt_tail` are the right policy boundaries to keep.
- `git diff --check main...HEAD` is clean.

