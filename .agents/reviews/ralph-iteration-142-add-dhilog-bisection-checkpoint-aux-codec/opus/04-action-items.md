# 04-action-items.md
## Action Items

### Critical
- None

### Important
- [ ] Reject `BISECTION_CHECKPOINT` records whose payload `checkpoint_icount` does not match the record header `icount`, and add a regression test.

### Suggestions
- [ ] Update the stale `build_kitchen_sink` doc comment in `golden.rs`.
- [ ] Decide and document whether future nested checkpoint versions should be replay-skippable AUX or hard parse failures.
