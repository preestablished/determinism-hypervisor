# Action Items

### Critical

None.

### Important

- [ ] **Decouple the timer guest from `ITERS_CMDLINE` (or document the
      dependence).** `boot()` passes `ITERS_CMDLINE = b"30000000"` to the
      timer guest, whose mode is selected from the first cmdline byte
      (`tests/nanokernel/asm/timer_guest.asm` lines 66–78: `'m'`/`'a'`/`'d'`).
      `'3'` matches none → default open-window path, byte-identical to the
      `b""` the other timer tests use, so the test is correct TODAY. Make it
      robust against a future `ITERS_CMDLINE` retune that could flip the
      guest into `mask`/`arm`/`defer` mode and silently change what the test
      proves. Preferred fix: let `boot()` accept the cmdline (or default the
      timer guest to `b""`) so it no longer borrows the landing-loop's
      iteration string. Minimum fix: a comment at
      `boot(nanokernel::timer_guest_elf())` (m4_transparency.rs:351) noting
      the leading byte is not a mode char. (See 01-I1.)

### Suggestions

- [ ] **Add a divergence-sensitivity child (inputs Y).** One extra fork in
      `frozen_parent_children_replay_*` running a different input set and
      asserting `out_c.state_hash != out_a.state_hash` — proves the test can
      fail and that the chain reflects the injected vectors. Highest-value
      cheap add. (S1)
- [ ] **Assert the frozen parent stays pristine.** Read the parent's
      `TIMER_GUEST_TABLE_GPA` count after A and B ran and assert it is 0 —
      the running-guest analogue of fork_engine.rs's CoW byte-isolation
      proof. (S2)
- [ ] **Comment the explicit `(count, vec)` cross-check** as the
      human-readable counterpart to the hash, so it isn't later "simplified"
      away as redundant with `state_hash`. (S3)
- [ ] **Rename/clarify `ITERS_CMDLINE`** or its doc to reflect that it also
      serves as the timer guest's (default-mode) cmdline. Subsumed by the
      Important item if Option 1 there is taken. (S4)
- [ ] **Bound `count` before allocating** the vectors `Vec` (or assert
      `count == 3` first) so a corrupt guest table fails cleanly instead of
      OOM-aborting. Defensive only. (S5)

### Non-actions (verified, no change needed)

- vns axis is 0-based counter-space for both children (`vt.rs:43`,
  `runctl.rs:312`) — identical for A and B by construction. No fix.
- counter `Some` reset (`restore_engine.rs:347`, step 6) is before the child
  runs; single-threaded; no race/flake window. No fix.
- `build_dhsnap` reads `&DetEntropy` only (`entropy.state()`); the twice-run
  closure has no hidden mutation order-dependence. No fix.
- File at 410 lines / 3 tests is cohesive; do NOT split (per the integration-
  testing research note on per-file link cost). No fix.
