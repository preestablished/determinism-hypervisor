# Suggestions

- Add an integration-shaped regression that composes `terminal_sdk_target_for_tail` with the tail acceptance/substitution predicate. The current helper-only test uses an icount combination that production target selection cannot currently supply.
- If the fix records the replay-observed SDK event icount, name that value distinctly from the recorded target icount. The current helper parameter name `terminal_event_icount` is easy to read as either recorded or observed.
- Keep the exact-end negative cases from the new unit test. They directly protect the bead's "do not hide exact final-state divergence" requirement.

