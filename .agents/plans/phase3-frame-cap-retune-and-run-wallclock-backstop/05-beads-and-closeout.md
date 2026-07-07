# Step 4 — Ledger Close: Resolutions, Beads, Push

## Resolution Files (three request dirs)

Write these as each work item lands, following the house pattern (see
`.agents/requests/rom-bridge-getframebuffer-region-contract/05-resolution.md`
for the shape: what was asked, what was done, evidence cites, what the
counterparty should do next). All paths below are under `.agents/`.

1. `.agents/requests/phase3-snapshot-restore-no-frame-under-no-tick-take-two/09-followup-resolution.md`
   — closes `08-followup-frame-hard-cap.md`: the retune commit hash, the
   fresh measurement, the cap arithmetic, the `DETCHANNEL_FRAME_HARD_CAP`
   determination (expected: fixture-only, unchanged), and the green-gate
   evidence dir. (Numbering continues the dir's existing `00-08` sequence.)
2. `.agents/requests/nextsdkevent-run-wallclock-backstop/01-resolution.md` — the
   empirical answer per `04-` (close-with-evidence or backstop-landed), the
   probe tests cited by name, and the note that the bridge can retire its
   `timeout(1)` stopgap (or how to set the deadline, if implemented).
3. `.agents/requests/phase3-frame-cap-retune-and-run-wallclock-backstop/04-resolution.md`
   — the consolidated resolution the requesters asked for: per-item outcome
   (handoff filed + guest-sdk ack status; caps retuned + green evidence;
   backstop question settled), so the bridge can respond with their
   `05-verification.md`.

Where guest-sdk's acknowledgment is pending at write time, say so explicitly
in `04-resolution.md` ("handback filed <date>, awaiting bead flip") rather
than blocking the resolution on it.

## Beads (this repo)

Per CLAUDE.md, bd for ALL task tracking. Suggested structure (adjust IDs to
what `bd create --silent` returns):

```bash
HANDOFF=$(bd create "Verify DHILOG/replay surfaces vs guest-sdk ext-hyp bead contracts and file handback" \
  -d "Contract-by-contract matrix per .agents/plans/phase3-frame-cap-retune-and-run-wallclock-backstop/02-. Handback into guest-sdk .agents/requests/ + bead notes. Gates Phase 3 exit gate 2." \
  -t task -p 0 -l analysis --silent)
RETUNE=$(bd create "Retune FRAME_HARD_CAP/LINUX_FRAME_HARD_CAP from fresh real-emulator measurement" \
  -d "Per plan 03-: measure instr/frame twice, retune with derivation comments, 3x green linux_m5 vs real image, evidence under target/. Closes take-two 08-followup." \
  -t task -p 1 -l testing --silent)
BACKSTOP=$(bd create "Settle NextSdkEvent wall-clock backstop question empirically (confirm-first)" \
  -d "Per plan 04-: probe A (idle HLT under no-tick) + probe B (non-HLT zero-retirement) as permanent tests; resolution into .agents/requests/nextsdkevent-run-wallclock-backstop/. Implement deadline only if hang reproduces." \
  -t task -p 2 -l testing --silent)
```

No dependencies between the three — they are independently landable. Claim
(`bd update <id> --claim`) when starting; close RETUNE and BACKSTOP with the
resolution-file cite as the reason. **HANDOFF closes only on guest-sdk's
acknowledgment** (bead state change or handback note — that is acceptance
criterion 3's bar): the handback request dir in their repo is the ack
trigger; if the ack hasn't landed by session end, leave HANDOFF open with a
"filed <date>, awaiting bead flip" note and record the same pending status in
`04-resolution.md`.

If step `02-` surfaces a contract divergence or step `04-` reproduces a hang,
file the follow-on bead at that moment (P0 for a divergence — it blocks the
handoff; the backstop implementation inherits the BACKSTOP bead).

## Commits

Group per work item (git-conventions: logical units, bodies explain why):

- Handoff: any new tests here + the two resolution touches; the guest-sdk
  handback commits in **that** repo (verify `pwd`/`git remote -v` first).
- Retune: the two test-file cap changes + derivation comments + take-two
  resolution file, one commit citing `08-followup-frame-hard-cap.md`.
- Backstop: probe tests + resolution file (+ implementation if it came to
  that).
- Final: `04-resolution.md` in the consolidated request dir.

## Session Close (mandatory, per CLAUDE.md)

In each repo touched (this repo, guest-sdk):

```bash
git pull --rebase   # BEFORE any local merge, never after (memory: ralph lesson)
bd dolt push
git push
git status          # must show up to date with origin
```

Quality gates before push: `cargo test -p dh-worker --release` fixture suites
green; `cargo clippy` clean on touched crates if that's the repo's standing
gate (check CI config); the ignored lab-lane tests are green per the evidence
dirs.

## Definition of Done

All boxes in `00-summary.md` §Exit Criteria checked; both repos pushed; the
bridge notified implicitly via `04-resolution.md` (they watch the request
dir per `03-verification-offer.md`).
