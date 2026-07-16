# Suggestions (non-blocking)

### S1. Distinguish "unknown key in lock" from "host drift"

`live_value`'s `*) echo "<unknown key>"` collapses a *config error* (a
typo'd or stale key in the lock, e.g. `cpu_modle_id=`) into the same
"host drifted" report as a genuine microcode change. Both fail closed,
which is correct, but the operator message points at the re-baseline
procedure (unhold/upgrade/reboot) when the real fix is "fix the lock."
Consider: if `got == "<unknown key>"`, emit a distinct
`::error::unknown lock key '$key' — fix ci/determinism-class.lock` and a
separate non-zero exit reason. Minor; the current behavior is safe.

### S2. CRLF guard inside the script (belt-and-suspenders to S2 in I3)

If you don't want a `.gitattributes`, the script could strip a trailing
`\r` defensively: `line="${line%$'\r'}"` right after the read. One line,
makes the comparator robust to a CRLF lock regardless of git config.
Pick this OR the `.gitattributes` approach, not necessarily both.

### S3. Nightly has no failure notification

`nightly-drift.yaml` surfaces failures only in the Actions tab. For a
tripwire whose whole point is catching silent host drift, a no-one-looks
window is plausible. Acceptable for now (the brief calls it out), but a
single `if: failure()` step posting to a webhook/issue would close the
loop. Low priority while a human watches the tab.

### S4. No concurrency guard on nightly-drift

`ci.yaml` has a `concurrency` block; `nightly-drift.yaml` has none. On a
single self-hosted runner the default is one-job-at-a-time, so a nightly
overlapping a push-triggered `kvm-intel` run will serialize (queue), not
collide. Harmless, but adding `concurrency: { group: nightly-drift,
cancel-in-progress: false }` documents the intent and prevents two
manual `workflow_dispatch` clicks from stacking. Cosmetic.

### S5. `landing_precision` in the canary — NOT recommended

Considered per the brief. The nightly's job is drift detection: the host
tuple plus a semantic divergence canary. `landing_precision` (~71s) tests
margin/skid *mechanics* — valuable, but it would catch a code regression,
not a host/KVM drift, which is what the nightly is scoped to. The
`regression` 1e9-twice test already catches "same kernel package, KVM
behavior changed" (the exact gap the host tuple can't see), and
`counting_semantics`/`counting_smoke` cover attribution. I found **no
concrete drift mode** that only `landing_precision` would catch. Leave it
in the per-PR `kvm-intel` lane (`cargo test --workspace` already runs it).
Keeping the nightly lean also shortens the serialize window on the single
runner (S4).

### S6. Multi-socket / multi-CPU note (informational)

`cpuinfo_field` uses `grep -m1` (first match = CPU0). Correct for this
single-socket i5-8400. If the determinism class ever moves to a
multi-socket box, a per-socket microcode skew would be invisible. Out of
scope for this hardware; worth a one-line comment so it isn't forgotten
at the next re-baseline.
