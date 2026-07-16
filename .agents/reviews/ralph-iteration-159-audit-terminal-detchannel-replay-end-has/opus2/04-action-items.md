# Action Items

1. Fix or justify the mismatch between `terminal_sdk_target_for_tail` returning `target.icount == header.end_icount` and `terminal_sdk_recorded_end_hash_substitution_allowed` requiring `target.icount < header.end_icount`.

2. Add an integrated regression test that derives the terminal SDK target via `terminal_sdk_target_for_tail` and proves the intended early-HLT terminal-tail normalization still works.

3. Add a regression proving exact-end terminal SDK tails with mismatching live end hashes fail instead of being substituted.
