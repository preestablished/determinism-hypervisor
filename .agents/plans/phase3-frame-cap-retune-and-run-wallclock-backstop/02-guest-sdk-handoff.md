# Step 1 — The guest-sdk Handoff (Phase 3 Exit Gate 2 Unblock)

Goal: verify every element of the two guest-sdk ext-hyp bead contracts against
what this repo ships, fix any real gap, and file evidence sufficient for
guest-sdk to flip both beads. This is mostly verification; write code only
where a contract element is genuinely missing or unexercisable.

## 1. Pin Down the Contracts

Read the authoritative contract text, not just the bead summaries:

1. From `~/git/preestablished/guest-sdk`: `bd show
   guest-sdk-ext-hyp-input-log-dev-events` and `bd show
   guest-sdk-ext-hyp-determinism-replay-linux` (both P0/BLOCKED, updated
   2026-06-18).
2. The authoritative contract text (already located; re-confirm before use):
   `~/git/preestablished/guest-sdk/docs/prompts/guest-sdk-in-guest-chain-milestones-3-5.md`
   — line 39 (host mutation recording: ring C/I pushes, ring A/W consumer
   bumps, `pio_answer`), line 53 (Ms3 input path:
   `InjectInputs(PAD_SET @ at_frame)` semantics), line 58 (Ms5 replay:
   checkpointed C/I producer sequences, same decisions at same inject points,
   synthesizer absent). Plus
   `~/git/preestablished/guest-sdk/.agents/requests/phase3-ms5-groundwork-while-blocked/`
   `01-current-state.md` and `02-requested-work.md` (lines 76-77), which
   enumerate the exact element list. Note: guest-sdk pins the contract at the
   **element/semantic level** — no byte-level payload-layout spec was found
   there. So the verification baseline is element coverage and semantic
   fidelity, not byte-diffing; if a layout doc does turn up, escalate to
   byte-level comparison for those elements.
3. Determine what "available to the Intel VM lane" means operationally: find
   guest-sdk's Intel-lane CI/test setup (their Ms5 `determinism_replay` gate
   plan, bead `guest-sdk-m5-determinism-replay-ci-gate`) and identify what
   they consume from this repo — a dh-worker binary? crates by path/git dep?
   the deployed worker at `/run/dh/grpc.sock`? Record the answer in the
   handoff; "the code exists on main" is not by itself lane availability.

## 2. Build the Verification Matrix

One row per contract element. For each: **code cite + test cite + (where
applicable) lab-lane evidence cite**. Starting points already verified in
`01-current-state.md` (re-confirm, then deepen):

| Element | Verify |
|---|---|
| `PAD_SET` encoding | `dhilog.rs:44,:171` writer + payload layout vs. guest-sdk's expected layout; replay application in `replay_engine.rs`; exercised by M5 tests |
| `DEV_EVENT` ring C/I pushes | `EVENT_RING_PUSH` emission at `detchannel.rs:797` fires for **both** C and I ring pushes; payload carries ring id + payload bytes per contract |
| `DEV_EVENT` ring A/W consumer bumps | `fn cons_bump` at `detchannel.rs:800` fires for **both** A and W; encoding matches; **and replay verifies/applies it** (`replay_engine.rs:611,:913`). This is the request's flagged question mark — give it the most scrutiny, incl. a targeted test if none covers A and W distinctly |
| `pio_answer` | `EVENT_PIO_ANSWER` writer `dhilog.rs:218`; replay strict-match divergences `pio_answer_missing`/`pio_answer_mismatch` (`replay_engine.rs:174,:185`); encoding unit test at `dhilog.rs:537` |
| Replay-mode input-log application, synthesizer absent | `replay_engine.rs` applies `PadSet`/`DevEvent` from the log with no live input source; cite the test proving replay consumes the log rather than devices |
| Bit-identical Linux replay gate | Linux M5 record-replay corpus gate: `linux_m5_record_replay_post_ready_corpus_reverifies` (`crates/dh-worker/tests/m5_record_replay.rs:123`) — M9 evidence `17-linux-m5-corpus.log` **plus a fresh rerun** on the current image (fold into the `03-` lab session; cite the new evidence dir) |

Method per row: read the code, run the covering test(s), and check the
semantics a test exercises against the guest-sdk contract text (element-level,
per §1.2 — byte-diff only if a layout spec surfaces). Greps prove existence;
only a test proves coverage.

## 3. Gap Handling

If a matrix row has no covering test or an encoding mismatch:

- **Missing test, code correct** → add the test in this repo (likely home:
  `crates/dh-inputlog` unit tests or `crates/dh-worker/tests/`), cite it.
- **Encoding mismatch vs. guest-sdk contract** → stop and assess direction of
  fix. The DHILOG format shipped through M9 acceptance and has replay evidence
  behind it; do not silently change wire encodings. If the mismatch is real,
  it is a cross-repo contract negotiation — record it in the handback as a
  divergence with a proposal, and file a bead here; do not unilaterally break
  compatibility.
- **Genuinely missing capability** → implement in this repo with tests, then
  cite. (Per `01-`, none is expected — cons-bump, the flagged one, exists.)

## 4. File the Handback

1. Write the matrix + lane-availability statement as a handback into
   guest-sdk's established structure: a new directory
   `~/git/preestablished/guest-sdk/.agents/requests/phase3-ext-hyp-input-log-and-replay-handoff/`
   (their existing request dirs are all `phase3-*`) containing the evidence,
   per-bead. Include: this repo's git rev, the workload image identity used
   for the replay-gate rerun, and the evidence-dir path.
2. Append a note to each guest-sdk bead from their repo:
   `bd update guest-sdk-ext-hyp-input-log-dev-events --append-notes="..."`
   (and the replay bead) pointing at the handback dir — **`--append-notes`,
   not `--notes`**: `--notes` overwrites, and both beads carry a standing
   unblock-condition NOTES blob that must survive. Do **not** close their beads —
   the unblock decision is theirs; the acceptance criterion is their
   acknowledgment (bead state change or handback note).
3. Commit + push in the guest-sdk repo too (`git` + `bd dolt push` there) —
   verify repo context (`pwd`, `git remote -v`) before committing across
   repos.

## Acceptance for This Step

- Every element of both bead contracts has a matrix row with citation or a
  landed gap-fix; ring A/W cons-bump has explicit per-ring evidence.
- Intel-VM-lane availability is explicitly stated with its operational
  meaning, not asserted.
- Handback filed in guest-sdk (files + bead notes), pushed.
- Bead here: create/claim a bead for the handoff work. Close it only once
  guest-sdk acknowledges (bead state change or handback note — acceptance
  criterion 3 requires the ack, not just the filing); if the ack hasn't
  landed by session end, leave the bead open with a note and record
  "handback filed, awaiting bead flip" in the resolutions (see `05-`).
