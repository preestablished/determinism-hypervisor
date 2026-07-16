# Action Items

### Critical

None. Merge is not blocked.

### Important

None.

### Suggestions

These are optional polish items; none block merge. File as low-priority follow-ups or fold
in opportunistically.

- **[S1] Tighten the `gettid` doc comment** in `tests/determinism/tests/regression.rs`. The
  middle sentences read as stream-of-consciousness. Replace with one clear sentence: std
  has no gettid wrapper, tests run on worker threads (tid != pid), so route overflow to
  this thread's real kernel TID via the syscall. Readability only; behavior is correct.

- **[S2] Add a P0-style assertion message** to `ten_million_twice_equal_final_hash`'s bare
  `assert_eq!(a, b)`, matching the loud framing on the 1e9 gate. Trivial.

- **[S3] Add a one-line comment at `ITERS_CMDLINE`** documenting the budget/HLT margin
  invariant: the guest must yield strictly more than `budget` retirable instructions
  (prologue + 1e9 loop + epilogue) so the run stops on `BudgetReached`, never
  `GuestHalted`. Defensive doc only — the code already fails loudly if violated.

### Out of scope (confirmed, no action here)

- Branch protection / required-for-merge wiring is tracked separately as bead 8n7. Correctly
  not part of this change.
