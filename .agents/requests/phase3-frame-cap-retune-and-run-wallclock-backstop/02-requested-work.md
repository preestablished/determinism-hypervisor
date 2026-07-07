# Requested Work

## What We Need (Behavioral)

1. **Frame-cap retune, derived not guessed.** Re-measure the real
   emulator's instructions/frame from the current reference-workload image
   (the trail holds ~25M from take-two and 27.8M from bead `38b6`'s newer
   note — trust your own measurement over both), then retune
   `FRAME_HARD_CAP` (`m5_frame_scheduling.rs:47`; the follow-up's concrete
   recommendation is ≈150M), `LINUX_FRAME_HARD_CAP`
   (`m5_net_loopback.rs:47`), and — if it gates the real-emulator path —
   `DETCHANNEL_FRAME_HARD_CAP` (`m5_frame_scheduling.rs:48`, currently
   1M), exactly as `08-followup-frame-hard-cap.md` asks. Leave a comment
   deriving each value from the measurement so the next emulator-cost
   change is a one-line retune. If you prefer image-profile-aware caps
   (fixture vs real) over larger constants, that's yours to decide.
2. **`linux_m5` green against the real image.** Run the frame-scheduling
   gate against the current reference-workload artifact (coordinate with
   refwork's `gp9` image rebuild if a fresher one is imminent; a green run
   against the newest `40eaf4f`-fixed image is the substance) and record
   it in a durable evidence file so exit-gate accounting can point at it.
3. **The guest-sdk handoff (highest leverage, mostly verification).** Take
   the two guest-sdk ext-hyp bead contracts
   (`guest-sdk-ext-hyp-input-log-dev-events`,
   `guest-sdk-ext-hyp-determinism-replay-linux`) item by item and verify
   each element is shipped **and exercisable from the Intel VM lane** —
   PAD_SET, DEV_EVENT for ring C/I pushes and ring A/W consumer bumps,
   `pio_answer`, replay-mode input-log application, the bit-identical
   Linux replay gate. Where coverage exists, cite the test/evidence; where
   it doesn't (the ring A/W consumer-bump encodings are the known
   question mark), implement the gap. File the result to guest-sdk —
   a note per bead or a short request-dir handback — sufficient for them
   to flip both beads and start Ms5.
4. **Settle the wall-clock question, confirm-first.** Step 0 is an
   empirical repro attempt of the *actual* suspect condition: a guest
   parked in idle HLT (IF=1) under no-tick — e.g. the agent blocked in
   `epoll_wait` with no pending timer — and separately a non-HLT
   zero-retirement spin, under `Run{until: NextSdkEvent}`. If neither
   hangs (terminal HLT already returns `GuestHalted`), close the request
   with that evidence and implement nothing. If a hang reproduces,
   implement the host-side deadline: parameter or worker config (document
   the default and the override path for bridge/orchestrator), a
   distinguishable gRPC status reported by the worker itself, slot left
   recoverable (DestroyVm/RestoreSnapshot works afterward), and the
   determinism constraint from `01-` held: host-side abort only, no
   guest-visible event, aborted runs never committed as replayable.
5. **Close the ledger.** Write resolution files into
   `requests/phase3-snapshot-restore-no-frame-under-no-tick-take-two/` and
   `requests/nextsdkevent-run-wallclock-backstop/` per the established
   pattern. This request directory closes the same way: append
   `04-resolution.md` here; we respond with `05-verification.md` after
   running our side (see `03-`).

## Suggested Sequencing (Yours To Overrule)

Item 3 first — it is mostly reading and testing, and it unblocks another
repo's P0 chain the moment it lands. Items 1 → 2 are one lab session.
Item 4's step-0 repro can interleave anywhere; its implementation branch
only if needed. Item 5 as each lands.

## Acceptance Criteria

1. All retuned caps carry measurement-derived values with derivation
   comments; the fixture-profile tests still pass unchanged. Empirical
   non-vacuousness check: with the retuned caps, the real-emulator test's
   normal outcome is `BUDGET_REACHED` (not `HARD_CAP`), and each cap stays
   within a stated small multiple (≤4×) of measured per-frame cost × the
   frame count it covers — a cap that can never fire is not a gate.
2. `linux_m5` frame-scheduling gate green against a real-emulator image,
   evidence file with git revs (this repo + workload image hash) recorded
   under `target/` with the usual timestamped-directory discipline.
3. Handoff evidence filed to guest-sdk covering every element of both bead
   contracts with a citation or a landed gap-fix, explicitly confirming
   Intel-VM-lane availability; guest-sdk acknowledges (bead state change
   or handback note).
4. The wall-clock question closed empirically, one of two ways:
   (a) repro attempt documented, no hang → resolution note with the repro
   harness cited; or (b) hang reproduced → backstop implemented, plus a
   regression test reproducing the *parked/idle-blocked* condition (not
   terminal HLT, which already stops) that returns the distinguishable
   status within the deadline and leaves the slot recoverable, **and** a
   record+replay determinism test showing backstop-enabled-but-not-fired
   runs are bit-identical to backstop-disabled runs.
5. Resolution files present in both open request dirs;
   `08-followup-frame-hard-cap.md` closed by the retune commit.

## Out Of Scope For This Request

- Play-60fps M4 (epoch-hash pipeline, bead `38b6`) — measured-and-deferred;
  needs an emulator speedup from reference-workload first.
- The READY-snapshot regeneration itself — reference-workload `refwork-gp9`
  + operator cutover territory; item 2 only consumes the freshest image
  available.
- Worker orphan-slot hardening (`determinism-hypervisor-umay`, bridge bead
  `72o`) — real, but separately tracked; don't fold it in here.
