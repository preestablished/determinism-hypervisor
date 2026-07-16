# Positive Notes

- Separating tail acceptance from end-hash substitution is the right direction. The new helper makes the hash-normalization policy easier to audit than the previous `terminal_sdk_target.is_some()` check.

- Excluding exact `BudgetReached` and exact `GuestHalted` end-boundary tails from substitution matches the bead's concern that a terminal SDK marker should not hide a real final-state divergence.

- The reseal comparison still requires matching generated detchannel records after position normalization; the change does not broaden payload-level equivalence for SDK or detchannel outputs.

- The added helper test covers the intended accept/reject matrix for the helper itself, including non-`BudgetReached`, no-tail, before-event, exact-end, and early-HLT cases.
