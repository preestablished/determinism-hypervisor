# Suggestions

### S1. Comparator silently drops a final line that lacks a trailing newline

`while IFS= read -r line; ... done < "$LOCK"` does not emit the last line if the
file has no terminating newline — standard `read` behavior. I verified this:

```
# lock with two keys, no trailing newline on the second
cpu_vendor=GenuineIntel\nhost_kernel=6.8.0-124-generic   (no final \n)
→ "ok: cpu_vendor=..." ; "determinism class matches the lock (1 keys)."  # host_kernel SKIPPED
```

The committed `ci/determinism-class.lock` **does** end in `\n` (verified via
`xxd`; the live run reports all 7 keys), so this is **latent, not active**.
But the failure mode is nasty: a future hand-edit (or an editor that strips the
final newline) that lands `host_kernel` as the last line would **silently stop
checking the kernel version** — the single most likely thing to drift on this
box — and the zero-keys guard would NOT catch it (still 6 keys > 0). A drift
tripwire that can be partially disarmed by a whitespace edit undercuts its own
purpose.

**Fix (one line):** use the classic terminator-tolerant idiom:
```bash
while IFS= read -r line || [[ -n "$line" ]]; do
```
Cheap, removes a class of silent-undercount bugs. Recommend taking it.

### S2. Consider asserting an expected key count, not just "> 0"

The zero-keys guard catches a totally-broken lock but not a *partially* parsed
one (see S1, or a future merge that drops keys). A determinism baseline has a
known, fixed schema (7 keys today). A `MIN_KEYS` / expected-key-set assertion
("lock must define exactly these keys") would turn "host_kernel silently
missing" from green into red. Lower priority than S1's one-liner, but it's the
more robust version of the same idea. Optional.

### S3. Nightly canary failure is silently red — add failure routing

`nightly-drift.yaml` has no notification/issue-creation step. For a determinism
product, a **silently red nightly is the worst outcome**: drift or a semantic
regression lands and nobody is paged until someone happens to look at the
Actions tab. The `workflow_dispatch` escape hatch is present (good for manual
re-runs), but nothing surfaces a *failure*.

**Concrete proposal (implementable later, separate bead):** add a final job
`gated on failure()` that opens-or-updates a pinned tracking issue, e.g.:

```yaml
  notify:
    needs: [determinism-class, determinism-canary]
    if: failure()
    runs-on: ubuntu-latest          # hosted; just needs the API, not /dev/kvm
    permissions:
      issues: write
    steps:
      - uses: actions/github-script@v7
        with:
          script: |
            const title = "nightly-drift: determinism gate RED";
            const body  = `Run ${context.runId} failed (${context.payload.head_commit?.id || context.sha}). See logs.`;
            // search for an open pinned issue by a marker label, comment if found else create+pin
```

Note this job must run on a **hosted** runner (`ubuntu-latest`) so a
down/red kvm box doesn't also take out the alarm, and the workflow then needs a
`permissions: issues: write` block (currently absent — fine until this is
added). Judge: Important-ish for the product, but genuinely a follow-up; the
core gate works without it. Filed as a suggestion, not a blocker.

### S4. Nightly has no concurrency group

`ci.yaml` has `concurrency: cancel-in-progress` to keep the single self-hosted
box from queueing stale work; `nightly-drift.yaml` has none. If a manual
`workflow_dispatch` overlaps the 03:17 cron (or a run somehow overruns 24h), two
nightly runs could contend for the one box. Low severity (daily cadence, short
suite), but adding a `concurrency: { group: nightly-drift, cancel-in-progress: true }`
matches the established pattern and costs nothing. Optional.

### S5. Wrong-host self-protection — already covered, document the reasoning

The script greps `/proc/cpuinfo` / `uname -r` of *whatever host it runs on*. The
workflow pins it to `runs-on: [self-hosted, kvm-intel]`, so it only ever runs on
the right box (good). The script itself has no hostname/runner-label guard — but
it doesn't strictly need one: `cpu_brand`, `cpu_family`, `cpu_model_id`, and
`microcode` collectively act as a host fingerprint, so a run on the wrong host
would trip drift and exit 1 anyway (fail-closed in the safe direction). I
reasoned this through and **no code change is warranted**. Only suggestion: a
one-line comment in the script noting "this is intended to run only on the
kvm-intel runner; the cpu_* keys double as the host fingerprint" would save the
next reader the same analysis. Optional/cosmetic.

### S6. 60-day cron auto-disable — escape hatch already present

GitHub auto-disables `schedule:` triggers after 60 days of *repo* inactivity,
and scheduled runs only fire on the default branch. Both facts are already
handled: this is a high-activity repo (ralph iterates daily), and
`workflow_dispatch: {}` is present so a human can always kick a run manually if
the cron ever lapses. No action needed — noting it so it's on the record that it
was considered. The header comment "Scheduled runs only execute on the default
branch" is accurate.
