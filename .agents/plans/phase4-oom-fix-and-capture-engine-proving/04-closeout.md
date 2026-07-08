# 04 — Closeout: Resolutions, Beads, Cross-Repo Notifications

## 1. Items-1–4 Tail Check (Do First — It May Already Be Closable)

Bead `9f3x` (the OOM incident) is IN_PROGRESS, waiting on either:

- the bridge's confirmation: their `l1w` closed after redeploying
  `c0337ab`+ and re-running eqb validation, or a handback note in
  `.agents/requests/run-with-frame-capture-memory-leak-oom/`; or
- the phases track's `05-verification.md` appearing in
  `.agents/requests/phase4-oom-fix-and-capture-engine-proving/`.

Check both (`ls` the two request dirs; check bridge beads via the
rom-operator-bridge repo at `../rom-operator-bridge` if reachable). If
either confirmation exists, `bd close determinism-hypervisor-9f3x -r "..."`
citing it. If not, update the bead notes with the date checked and
leave it open — do not block item 5 on this.

## 2. Update The Request-Dir Resolution

`04-resolution.md` currently ends with "Item 5 … WAITING on
refwork-gp9". Per the handback shape in `03-verification-offer.md`,
the phases track owns `05-verification.md` — so do NOT write a file
with that name. Instead:

- Append an item-5 addendum section to `04-resolution.md` (dated), or
  write `04a-item5-resolution.md` — either way it must carry: the item-5
  bead id, proof commit(s), the evidence paths (committed sample set +
  local `target/` dir), which surfaces were proven (both, or which one
  and why), the four check outcomes (a)–(d) including the proven
  `layout_version`, the per-capture cost numbers, and the explicit
  "capture under concurrent RunWithFrameCapture remains unproven"
  scope note.
- If a real engine defect was found and fixed during proving, name the
  defect bead and fix commit.

## 3. Notify The Downstream Consumer

reference-workload's corpus request consumes this proof. After the
resolution lands, write a short pointer note into
`../reference-workload/.agents/requests/phase4-real-capture-corpus-fast-follow/`
(e.g. `04-engine-proof-available.md`): the evidence path in this repo,
the proven surfaces, `layout_version`, and the worker build to capture
against. Their request explicitly says "consume it, don't rebuild it" —
give them the pointer so they can. Commit and push in *their* repo
(verify `pwd`/`git remote -v` first — cross-repo commit discipline).

Also honor the standing warning: if their capture/corpus session might
run against a lab worker that predates `c0337ab`, restate the
segment-bounded-runs requirement in the note. (If the lab worker is
already on `c0337ab`+, say that instead.)

## 4. Beads Hygiene

- Close the item-5 bead with the proof evidence in the close reason.
- If proving surfaced follow-up work that is real but out of scope
  (e.g. capture-under-stream proving, out-of-bounds validation
  hardening, a cost number that threatens scorer M4's 1.5 ms budget),
  file beads now — P2/P3 with the evidence linked — rather than leaving
  them as prose in the resolution.
- Do not touch `jyo7`/`i74w`/`38b6` beyond what proving actually
  revealed about them.

## 5. Session Close (Mandatory, Per CLAUDE.md)

```bash
cargo test --workspace --release          # quality gate if code changed
git status && git add <files> && git commit
git pull --rebase
bd dolt push
git push
git status   # MUST show up to date with origin
```

Commit grouping suggestion: (1) the lab-lane proving test + any engine
fix, (2) the evidence + resolution + plan-status updates, (3) the
reference-workload pointer note (separate repo, separate commit). Mark
this plan's `00-overview.md` with an EXECUTED banner (see
`.agents/plans/run-with-frame-capture-memory-leak-oom/00-overview.md`
for the convention) once done.

## Acceptance Recap (Mapping To The Request's ACs)

- AC4 (item 5): sample capture evidence recorded — spec + hash table +
  cross-check log + revs — in the request dir; both surfaces proven or
  the single-surface caveat stated; negative `layout_version` case
  recorded.
- AC3 tail: `9f3x` closed on bridge/phases confirmation, or its notes
  updated with the check date.
- ACs 1, 2, 5: already satisfied by the 2026-07-07 execution — verify
  nothing in this session regressed them (the record/replay and RSS
  guards still green if the proving work touched worker code).
