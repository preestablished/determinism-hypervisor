# Action Items — ralph/iteration-56-runner-finishing (2nd reviewer)

### Critical
_None._

### Important

1. **At merge time, guard the PR #1 reachability flip.** PR #1's head SHA (`4156155365d5425b7e098f1beede573fa7ee9cca`) is currently IDENTICAL to this branch's tip, so the `--no-ff` merge to `main` will auto-mark PR #1 as **merged** by reachability — no cleanup needed in the as-is case. BUT this depends on the SHAs staying equal. Do this:
   - **If merging as-is** (branch tip still `4156155`): merge + push, then verify `gh api repos/preestablished/determinism-hypervisor/pulls/1 --jq .merged` returns `true`. Also delete the now-redundant `origin/chore/6eb-runner-finishing`.
   - **If ANY commit lands on this branch before merge** (including action item #2 below, a `/fix-review` auto-fix, or a rebase): the head SHAs diverge and PR #1 will be left **dangling OPEN** against a never-merged head. After merging+pushing main, explicitly run `gh pr close 1 --comment "Merged via ralph iteration 56 (--no-ff); head superseded."` and delete the chore branch.

2. **Fix the stale `--preflight` line in `docs/ops/github-runner.md:28-30`.** The text says the end-to-end perf assertion is `dh-workerd --preflight` "once it lands" — it HAS landed (`crates/dh-worker/src/preflight.rs`, binary `crates/dh-worker/src/bin/dh-workerd.rs`) and passes 17/17 live on this box. Drop "once it lands"; state preflight now re-asserts the §7.4/§2.1 host contract at startup, while keeping the honest note that neither `--verify` nor preflight performs a live `perf_event_open` syscall. Suggested replacement:
   > `--verify` checks the sysctl (a proxy — neither it nor `dh-workerd --preflight` performs a live `perf_event_open`); `dh-workerd --preflight` (now landed) re-asserts the §7.4/§2.1 host contract at service startup, 17 checks, green on this box.
   The file is already open in this iteration — folding it in is cheap. (If you do, see action item #1's divergence branch.)

### Suggestions

3. **Generalize the `kvm-intel-nightly-drift` literal group guidance.** Add one sentence to the caveat: a *second* measurement workflow should share the SAME concurrency group as `nightly-drift` (not a per-workflow group) so the two serialize, since `cancel-in-progress: false` only queues — it does not cross-workflow-exclude. Clarify that the single runner already gives mutual exclusion; the group only adds queue hygiene/ordering. (`02` S-1)

4. **Optional YAML polish.** Make the inline comment in `nightly-drift.yaml` reference the ci.yaml asymmetry directly, so the workflow file is self-explanatory without the doc. (`02` S-2)

5. **Optional follow-up (out of scope for this bead).** A tiny CI lint asserting the concurrency-policy split (`nightly-drift` = `false`, `ci` = `true`) would prevent a future edit from silently inverting the contract the doc now promises. File as a separate issue if valued. (`02` S-3)
