# 04-action-items.md
## Action Items

### Critical
- None

### Important
- [ ] Reject `BISECTION_CHECKPOINT` records when payload `checkpoint_icount` differs from the record header `icount`, and add a negative reader-validation test.

### Suggestions
- [ ] Strengthen the hand-pinned bisection checkpoint byte test with literal offset/byte assertions.
- [ ] Document or validate the intended codec responsibility for `max_covered_gap` consistency.
