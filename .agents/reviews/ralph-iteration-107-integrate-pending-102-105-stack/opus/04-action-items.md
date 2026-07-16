## Action Items

### Critical
- None.

### Important
- [ ] Fix `DestroyVm` lifecycle ordering so runtime removal and slot release cannot diverge.

### Suggestions
- [ ] Document or constrain `RuntimeTable::with` and `with_mut` before they run blocking KVM work.
- [ ] Reject duplicate slot-core ids.
- [ ] Make the live pinning test cpuset-aware.
- [ ] Avoid unlinking non-socket UDS paths.
