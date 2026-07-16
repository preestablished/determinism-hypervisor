# Positive Notes

- **Matches the M3 acceptance wording precisely.** The plan says "run nanokernel 1e9
  instructions twice from cold boot with fixed seed, final state hash equal." The test
  does exactly that, and goes further by comparing the full outcome 5-tuple
  (icount, rip, rcx, vns, state_hash) rather than just the endpoint hash.

- **The gate is stronger than the endpoint hash alone.** With the default 50M epoch grid
  (`DEFAULT_EPOCH_LEN`, `HashEpochs::EpochsOn`), the 1e9 run folds 20 intermediate epoch
  hash links into the chain. A divergence at any epoch boundary — not just at retirement
  1e9 — is caught. The doc comment's "20 intermediate epoch hash links" claim checks out.

- **Genuine cold boot.** Every `cold_run` rebuilds KvmSystem, slot/VM, ELF load, counter,
  and hash chain from scratch with identical fixed seeds. There is no warm-state shortcut
  that could mask a determinism bug.

- **Loud failure design.** `assert_eq!(out.reason, BudgetReached)` plus
  `assert_eq!(out.boundary.icount, budget, "landed exactly")` mean a guest that parks early
  (GuestHalted) or lands off-budget fails loudly rather than silently passing. The P0
  framing on the main assertion message is exactly right for a required-for-merge gate.

- **Correct hardware gating.** Self-skip on `/dev/kvm` (with a panic on *unexpected* probe
  errors, so a misconfigured runner is not silently skipped) is the right posture: runs in
  the kvm-intel lane and on the lab box, skips cleanly on hosted lanes.

- **Correct arm-lane exclude.** `determinism-tests` links x86-only `dh-vmm`; the arm lane
  already excludes `dh-vmm`, so excluding `determinism-tests` is both correct and required
  for the arm build to link. The one-line ci.yaml change is the minimal correct fix.

- **Clean, minimal footprint.** +157/-1 across 6 files, a self-contained workspace member,
  no churn in production crates. clippy clean, fmt clean, full workspace green.

- **Not flaky.** The 1e9 gate passed 3/3 on repeated live runs here; release and debug both
  pass. Timing (~4s debug) is well within a sane CI budget.

- **Honest accounting comments.** Both the test and `landing_loop.asm` carry candid notes
  about the per-thread counter routing and the cmdline-dependent prologue offset — useful
  for the next person who touches the margin.
