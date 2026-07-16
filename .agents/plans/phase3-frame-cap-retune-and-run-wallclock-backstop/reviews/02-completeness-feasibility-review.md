# Review 2 — Completeness / Feasibility (subagent, 2026-07-07)

Lens: can a fresh coding agent execute the plan without guessing, and does it
cover all 5 request acceptance criteria without dilution?

## Findings

1. **IMPORTANT** — 03 §1 hedged "adjust the filter to the actual test names".
   Actual names: `linux_m5` matches TWO tests in `m5_frame_scheduling.rs` —
   `linux_m5_frame_budget_records_post_ready_frame_marks` (:55) and
   `linux_m5_real_emulator_nop_game_frame_budget_diagnostic` (:240, also uses
   `FRAME_HARD_CAP` :276); `m5_net_loopback.rs`'s Linux test is
   `linux_pvblk_io_loopback_records_and_replays` (:54), which prints
   `run_icount` — the 1-frame measurement source. Pin all three.
2. **IMPORTANT** — the Linux replay corpus gate was required but never named:
   it is `linux_m5_record_replay_post_ready_corpus_reverifies`
   (`crates/dh-worker/tests/m5_record_replay.rs:123`). Name it in 02 and 03.
3. **IMPORTANT** — 02 §1 pointed at a grep when the contract docs are known:
   `guest-sdk/docs/prompts/guest-sdk-in-guest-chain-milestones-3-5.md`
   (lines 39/53/58) + `.agents/requests/phase3-ms5-groundwork-while-blocked/`
   (`01-`, `02-` lines 76-77). No byte-level layout spec exists in guest-sdk —
   the baseline is element/semantic coverage, which simplifies the matrix
   method.
4. **MINOR** — proposed handback dir name broke guest-sdk's `phase3-*`
   naming pattern → `phase3-ext-hyp-input-log-and-replay-handoff/`.
5. **MINOR** — `bd update --notes` OVERWRITES; both target beads carry a
   standing unblock-condition notes blob → use `--append-notes`.
6. **MINOR** — "self-skips elsewhere" is wrong without `DH_M9_ALLOW_SKIP=1`
   (`common/mod.rs:61,:169` — missing artifacts/dirty-ring otherwise error;
   ALLOW_SKIP not accepted for final gates). The five DH_M9_* vars listed
   exactly match `M9_LINUX_ARTIFACT_ENV_VARS`; `DH_M9_GUEST` exists but is
   unused by these m5 tests.
7. **MINOR** — AC3 requires guest-sdk's *acknowledgment*; the plan told the
   agent to close the HANDOFF bead on filing → close only on ack, else leave
   open with "filed, awaiting bead flip".
8. **NIT** — `bd create` commands lacked `-l` labels (house convention).
9. **NIT** — same `common/mod.rs:609` mis-cite as review 1 finding 2
   (actual: `assert_m9_real_emulator_initramfs` at :248, called :649).
10. **NIT** — bare `requests/...` path shorthand; spell `.agents/requests/`.

## Verified correct

All 5 acceptance criteria traced to plan coverage without dilution;
resolution-file numbering correct in all three request dirs (take-two →
`09-`, wallclock → `01-`, consolidated → `04-`); both guest-sdk beads exist
as claimed (P0/BLOCKED, 2026-06-18); raising the caps cannot break fixture
tests (no `HardCap`-stop assertions exist with these constants);
out-of-scope discipline holds (M4 epoch-hash, snapshot regen, orphan-slot
hardening all excluded); house plan style and session-close protocol match.

## Verdict

Strong, well-grounded plan; gaps were concentrated in "the plan knows but
didn't write down" executability details (findings 1–3, 5). No blockers.
