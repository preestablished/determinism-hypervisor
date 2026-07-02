# Deployed Verification (rom-operator-bridge side)

2026-07-02. We reviewed `5698d7e` (code, regression test, decision record —
all good; the capture-path determinism fix exceeds the original ask),
independently re-ran `framebuffer_layout_contract_is_enforced` (green), and
deployed it.

## Deployment Notes

- Your working tree had unrelated in-progress edits
  (`crates/dh-worker/src/m9_handoff.rs`, `Cargo.lock`), so we built from a
  clean worktree at `ff1e88c`: `~/git/preestablished/.dh-clean-ff1e88c`.
  **The running `dh-workerd` binary lives in that worktree's
  `target/debug/` — do not remove the worktree while it is deployed.**
- Restart followed the pid-file procedure; all slots were empty beforehand;
  snapstore untouched. Pid file updated.

## Result: Fix Confirmed, New Blocker Is Snapshot Content

`GetFramebuffer` via the bridge (`/api/frame/current`) now fails with your
new, precise error — logged by the bridge on every poll:

```text
GetFramebuffer framebuffer region layout_version 1 expects 229376 bytes, got 4096
```

So the layout_version plumbing works end-to-end, and the wrong-length
rejection names the offender exactly as specified. Acceptance criterion 4
cannot pass yet for a reason outside this fix's scope: **the guest inside
the rom-bridge-o73 READY snapshot publishes a 4096-byte framebuffer region**,
not a D7 229,376-byte one — it is not running the reference workload (that
is Phase 3, "workload in the box"). This also retroactively explains the
original symptoms: the old descriptor parse was reading the first bytes of a
4 KiB stub region.

## What We'd Ask Next (No Action Required Yet)

Criterion 4 closes when a READY snapshot whose guest publishes a conformant
`layout_version 1` framebuffer region exists — i.e., Phase 3 refwork-in-VM
territory, presumably a regenerated handoff snapshot. Until then the bridge
will render a calm "no frame yet" state for this failure (bridge-side work,
tracked in `rom-operator-bridge-9z2`). Nothing further needed from this repo
for the original request; consider it resolved on your side.
