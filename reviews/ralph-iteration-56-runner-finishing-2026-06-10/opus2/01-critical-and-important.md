# Critical & Important

## CRITICAL — none

No correctness, safety, or security defects in the diff.

---

## IMPORTANT-1 — PR #1 merge-by-reachability: clean, BUT the SHA equality is load-bearing (process guard, not a code defect)

**Finding (verified):** PR #1 (`chore/6eb-runner-finishing` → `main`, OPEN, MERGEABLE) has head SHA:

```
gh api .../pulls/1 --jq .head.sha  →  4156155365d5425b7e098f1beede573fa7ee9cca
git rev-parse HEAD (ralph/iteration-56-runner-finishing)
                            →  4156155365d5425b7e098f1beede573fa7ee9cca   # IDENTICAL
```

Both `chore/6eb-runner-finishing` (PR #1's head ref, also on origin) and `ralph/iteration-56-runner-finishing` point at the *same commit object*. The single commit's parent and the merge-base with main are both `b65dc96`.

**Reasoning on the merge.** GitHub marks a PR as `merged` (not just `closed`) when the PR's *head commit* becomes reachable from the *base branch* — it keys off commit reachability, not the local branch name that produced the merge. The ralph merge plan does:

```
git merge --no-ff ralph/iteration-56-runner-finishing -m "ralph: iteration 56 merge - ..."
```

into `main`. That merge commit's second parent is `4156155`, which makes `4156155` an ancestor of `main`. Because `4156155` IS PR #1's head SHA, GitHub will flip PR #1 to **merged** automatically on the next push of `main` — even though the merge was driven from a *differently named* branch. **This is the clean outcome; no `gh pr close 1` cleanup is needed.** (Confirmed `4156155` is not yet reachable from local/origin main — the flip happens at merge+push time, as expected.)

**The caveat that makes this IMPORTANT rather than a positive note.** The merged-not-dangling outcome depends entirely on the head SHAs being *exactly* equal. They are equal *right now* because this iteration adopted the pre-existing chore commit verbatim and the ralph loop added nothing on top (no review-fix, no gemini-ui commit). If, before the merge, anything lands a new commit on `ralph/iteration-56-runner-finishing` (a `/fix-review` auto-fix, the preflight doc fix recommended below, a rebase), then:

- PR #1's head stays at `4156155`,
- the branch tip moves to `4156155'`,
- the merge makes `4156155'` reachable but **not** `4156155`,
- → PR #1 is left **OPEN against a head commit never merged**, i.e. the messy dangling-PR-against-a-deleted-branch state.

**Action (process, do this at merge time):**
- If you merge the branch *as-is* (SHA still `4156155`): nothing to do — PR #1 auto-merges. Verify post-push with `gh api .../pulls/1 --jq .merged` → expect `true`.
- If ANY commit lands on this branch before merge (including the stale-doc fix below): after merging+pushing main, explicitly close PR #1: `gh pr close 1 --comment "Merged via ralph iteration 56 (--no-ff); head commit superseded — closing the chore PR."` and delete `origin/chore/6eb-runner-finishing`.

Either way this is a checklist item, not a blocker on the diff.

---

## IMPORTANT-2 — Stale doc line in the same file: `dh-workerd --preflight` "once it lands" — it has landed and passes 17/17

`docs/ops/github-runner.md:28-30` (NOT part of this diff, but in the file being edited):

> `--verify` checks the sysctl (a proxy — it does not perform an actual `perf_event_open`); the real end-to-end assertion is `dh-workerd --preflight` once it lands.

**Verified false as of this session.** `dh-workerd --preflight` is implemented (`crates/dh-worker/src/bin/dh-workerd.rs` dispatches `--preflight` → `dh_worker::preflight::run_preflight()`; logic in `crates/dh-worker/src/preflight.rs` with a hardware-gated acceptance test `full_preflight_passes_on_configured_host`). I ran it live on this box:

```
$ cargo run -p dh-worker --bin dh-workerd -- --preflight
ok   cpu.family ... ok   perf_event_paranoid  got [1] want [1] ... ok   kvm.slot_vm_smoke
preflight OK            # 17 checks, all ok
```

Note `perf_event_paranoid` is among the checks but is still a *sysctl* assertion — preflight does not yet do an actual `perf_event_open` either, so the "(a proxy ...)" nuance for `--verify` is partly still true. The precise fix: drop "once it lands" and say preflight now exists and re-asserts these at service start (it landed), while keeping the honest note that neither `--verify` nor preflight performs a live `perf_event_open` syscall.

Suggested wording:
> ... `--verify` checks the sysctl (a proxy — neither it nor `dh-workerd --preflight` performs a live `perf_event_open`); `dh-workerd --preflight` (now landed) re-asserts the §7.4/§2.1 host contract at service startup, 17 checks, green on this box.

Fold into this iteration since the file is already open. Not blocking the concurrency change.
