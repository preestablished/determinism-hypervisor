# Positive Notes

### P-1. The `<=1` paranoid threshold is *exactly* matched to this specific attr — not arbitrary, not over-conservative

This is the subtle part the change gets right. The attr is opened with
`pid=0, cpu=-1` (**per-process**, not CPU-wide) and `exclude_kernel=0` (it
measures the guest kernel). Mapping to kernel `perf_event_paranoid` semantics:

- Level **1** restricts *CPU-wide* event access for non-CAP_PERFMON users —
  but this event is **per-process**, so level 1 does **not** deny it. Test
  should run. `1 <= 1` → true. ✓
- Level **2** restricts *kernel profiling* for non-CAP_PERFMON users. Because
  `exclude_kernel=0`, this attr **does** request kernel measurement, so at
  level 2 an unprivileged open returns **EACCES**. Test must skip. `2 <= 1`
  → false. ✓
- Levels **3/4** (distro hardening): deny all unprivileged `perf_event_open`.
  Skip. ✓ — this is the GitHub-hosted-runner case (level 4) the fix targets.

So `<=1` is the *tightest correct* threshold: one higher (`<=2`) would try the
open at level 2 and eat an EACCES the gate is supposed to prevent; one lower
(`<=0`) would needlessly skip on a perfectly capable level-1 lab box. The
comment even cites why the lab box is at 1 (§7.4). Genuinely well-reasoned, and
it lines up with the module's own `CounterError::Open` doc note ("EACCES usually
means perf_event_paranoid is too strict (§7.4 sets it to 1)").

### P-2. `let-else` + `unwrap_or(i32::MAX)` is fail-*closed* throughout

Both failure exits are conservative: file unreadable → `return false` (skip);
unparseable contents → `i32::MAX` → skip. There is no path where a perf-policy
problem causes the test to *attempt* the open and red the lane. That is the right
default for a skip-guard, and it is consistent.

### P-3. The fmt-scope fix correctly identifies a real cargo footgun

`cargo fmt --all` formatting **local path dependencies** is a genuine, easily
overlooked behavior (`cargo fmt --help`: "Format all packages, and also their
local path-based dependencies"). With sibling repos wired in as path deps and
"sibling HEAD wins, no rev pinning" (ci.yaml header comment), `--all` really
would let an unrelated checkout's formatting red this repo's CI. The intent is
correct and the inline comment explains the *why* well — future maintainers
won't "simplify" it back to `--all`.

### P-4. The self-hosted security posture (unchanged but re-validated) is correct

Not touched by this diff, but worth recording since the prompt flagged it: the
`kvm-intel` self-hosted job is gated with
`if: github.event_name == 'push' || github.event.pull_request.head.repo.full_name == github.repository`,
which is exactly the fork-PR fencing the research doc prescribes for self-hosted
runners on public repos. `permissions: contents: read` is minimal, and
`push:` is scoped to `branches: [main]` to avoid the double-run pitfall. The fmt
change does not weaken any of this.
