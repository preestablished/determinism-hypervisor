# Request: Retune The Frame Caps, Settle The Run Wall-Clock Backstop, And Hand Off The Input-Log/Replay Surfaces To guest-sdk

## Who Is Asking

The `rom-operator-bridge` project (the Phase 3 validation surface) jointly
with the phases track, consolidating three loose ends into one executable
request. Filed 2026-07-07.

## Why determinism-hypervisor, Why Now

This repo's milestone work is **done** — M0–M9 all accepted (final M9
evidence at `target/m9-final-acceptance-20260621T004402Z/`), the capture
engine and D7 framebuffer contract shipped, and the play-60fps plan's
M1–M3 (`RunWithFrameCapture`) merged at `bdd476b`. But three items still
stand between this repo and a clean Phase 3 ledger:

1. **The frame-cap retune.** The take-two no-frame investigation's
   `07-handoff-resolution.md` localized the real-emulator no-frame failure
   to reference-workload (`refwork-4qj`, since fixed at refwork `40eaf4f`);
   its surviving follow-up `08-followup-frame-hard-cap.md` records that the
   only remaining gate is this repo's own fixture-calibrated test caps —
   `FRAME_HARD_CAP = 50_000_000`
   (`crates/dh-worker/tests/m5_frame_scheduling.rs:47`) while the real
   emulator costs ~25M instructions/frame (the follow-up recommends
   ~150M), so a three-frame budget overruns the cap by design, not defect.
2. **The wall-clock backstop question** —
   `nextsdkevent-run-wallclock-backstop/` (filed 2026-07-05 by
   rom-operator-bridge from guest-sdk's boot-scheduling-deadlock resolution
   action item #3; no resolution file yet). The filing is **confirm-first**:
   the run loop already stops on terminal HLT (`GuestHalted`), and the open
   question is whether an *idle* HLT (IF=1, no pending tick) or a non-HLT
   zero-retirement block can wedge `Run{until: NextSdkEvent}` inside
   `KVM_RUN`. Confirm it, and either close with evidence or implement the
   host-side deadline.
3. **The unissued handoff to guest-sdk** — the item with the most
   critical-path weight. guest-sdk carries two P0 BLOCKED beads
   (`guest-sdk-ext-hyp-input-log-dev-events`,
   `guest-sdk-ext-hyp-determinism-replay-linux`, both last updated
   2026-06-18 — *before* your M9 acceptance) that gate their Ms3 input
   acceptance and the Ms5 `determinism_replay` CI gate, i.e. **Phase 3
   exit gate 2**. The capabilities appear to already exist here (DHILOG
   `PAD_SET`/`DEV_EVENT` incl. `pio_answer`; replay-engine application;
   the Linux M5 record-replay corpus gate in the M9 evidence) — but nobody
   has verified coverage against the beads' contracts and told guest-sdk.
   Until that handoff lands, the phase's exit gate 2 sits blocked on a
   notification.

## The Ask In One Paragraph

Retune the frame-budget test caps from fixture calibration to
measurement-derived values (re-measure instructions/frame yourself — the
trail has both ~25M and a newer 27.8M figure — and cover
`LINUX_FRAME_HARD_CAP` in `m5_net_loopback.rs` and, if it gates the real
path, `DETCHANNEL_FRAME_HARD_CAP` too), re-run the `linux_m5`
frame-scheduling gate against the real reference-workload image so it is
durably green; empirically settle the idle-HLT hang question and implement
the wall-clock backstop only if the hang is real; and verify the DHILOG /
replay surfaces against guest-sdk's two ext-hyp bead contracts on the
Intel VM lane, filing the evidence back to guest-sdk so those beads can
unblock — then write resolution files into both open request directories.

## Files In This Request

| File | Contents |
|---|---|
| `01-current-state.md` | Evidence: what M0–M9 delivered, the three open items and where they are recorded |
| `02-requested-work.md` | The ask, sequencing, acceptance criteria, out of scope |
| `03-verification-offer.md` | What the bridge runs against the deployed worker once these land |
