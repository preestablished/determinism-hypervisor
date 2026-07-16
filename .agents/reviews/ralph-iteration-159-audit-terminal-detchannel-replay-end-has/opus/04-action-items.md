# Action Items

- Fix the integration mismatch so early `GuestHalted` terminal SDK tails under recorded `BudgetReached` can still use the recorded end hash only after the matching SDK event was observed.
- Add a regression test that uses the same target-selection path as production and proves both sides of the requirement: early `GuestHalted` terminal SDK tails are accommodated, while exact-end final-state mismatches are not substituted.

