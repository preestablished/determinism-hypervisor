# Positive Notes

- **It actually holds under torture.** 5/5 sequential 1e9 runs, serial + parallel, full workspace x2 — all green, zero flakes. For a P0 determinism gate, the empirical stability is the headline and it earned it.

- **The gate checks the trajectory, not just the endpoint.** Asserting the full `(icount, rip, rcx, vns, state_hash)` tuple with `HashEpochs::EpochsOn` folds 20 intermediate 50M-epoch links into the compared hash. A bug that diverged mid-run but happened to reconverge by 1e9 would still be caught. Stronger than a bare final-hash compare, and the doc comment (regression.rs:100-102) explains exactly this.

- **Exact landing is asserted, not approximated.** `assert_eq!(out.boundary.icount, budget, "landed exactly")` plus `StopReason::BudgetReached` means the test fails loudly if the boundary engine overshoots/undershoots, rather than papering over skid. Good — the icount precision is itself part of the determinism claim.

- **The 1e7 smoke is well-chosen.** 10M < 50M epoch_len, so it crosses zero epoch boundaries (one final link only) and runs sub-second — a genuinely fast local iteration path that still exercises the same `cold_run` rig. Clear separation of the slow gate from the fast smoke.

- **Self-skip is honest.** `kvm_usable()` distinguishes NotFound/PermissionDenied (skip) from unexpected errors (panic) — it won't mask a real probe failure as "no kvm." And because the kvm-intel CI lane pre-checks `/dev/kvm` rw and fails if absent, the skip path can't quietly hide a broken gate on the box.

- **CI lane separation is principled.** arm excludes the x86-only crate; the Intel self-hosted lane runs the live suite and guards against fork PRs (`if:` repo-owner check) and against silent skips (rw probe). The arm-exclude addition is minimal and consistent with the pre-existing exclude pattern.

- **Fixed-seed discipline is explicit and consistent.** `[7;32]` is used for base image hash, kernel hash, and both StateHashChain seeds, with an inline comment noting it's "identical across the two runs." No hidden entropy in the test setup.

- **Clean compile.** Zero warnings on a forced rebuild despite the empty `[dependencies]` table and the no-op `#[allow(unsafe_code)]`.
