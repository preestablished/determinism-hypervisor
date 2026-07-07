# Plan: Frame-Cap Retune, Run Wall-Clock Backstop (Confirm-First), guest-sdk Handoff

Resolves `.agents/requests/phase3-frame-cap-retune-and-run-wallclock-backstop/`
(filed 2026-07-07 by rom-operator-bridge + phases track). Plan verified
against `main` @ `4497f60` (the request text cites `bdd476b`, a few commits
behind); M0–M9 accepted. This plan turns the request's three consolidated
loose ends into an executable sequence for a coding agent.

## The Three Deliverables

1. **guest-sdk handoff (do first — highest leverage).** Verify, item by item,
   that the DHILOG/replay surfaces cover the contracts in guest-sdk's two P0
   BLOCKED beads (`guest-sdk-ext-hyp-input-log-dev-events`,
   `guest-sdk-ext-hyp-determinism-replay-linux`), confirm Intel-VM-lane
   availability, fix any gap, and file the evidence back to guest-sdk so both
   beads can flip. This gates **Phase 3 exit gate 2**. → `02-guest-sdk-handoff.md`
2. **Frame-cap retune + durable `linux_m5` green.** Re-measure real-emulator
   instructions/frame, retune `FRAME_HARD_CAP` / `LINUX_FRAME_HARD_CAP`
   (and `DETCHANNEL_FRAME_HARD_CAP` only if it gates the real path — evidence
   says it does not), with derivation comments; run the `linux_m5` gates green
   against the real reference-workload image and record evidence under
   `target/`. → `03-frame-cap-retune-and-linux-m5-gate.md`
3. **Wall-clock backstop, confirm-first.** Empirically test whether an idle
   HLT or a non-HLT zero-retirement block can wedge `Run{until: NextSdkEvent}`
   inside `KVM_RUN`. Analytic prior (see `01-current-state.md`): this VMM has
   **no in-kernel irqchip**, so every HLT exits to userspace and stops the run —
   the expected outcome is *close with evidence, implement nothing*. Implement
   the host-side deadline only if a hang actually reproduces.
   → `04-wallclock-backstop-confirm-first.md`

Then close the ledger: resolution files in **three** request dirs + bead
hygiene + push. → `05-beads-and-closeout.md`

## Sequencing

The request suggests item 3 (handoff) first and we agree — it is mostly
reading/testing and unblocks another repo's P0 chain immediately. Concretely:

1. `02-` handoff verification (needs the M9 lab lane for the replay-gate rerun;
   can share the lab session with step 2).
2. `03-` measurement + retune + green gate (one lab session; the measurement
   run doubles as handoff evidence for the replay gate).
3. `04-` step-0 repro (interleave anywhere; small, self-contained).
4. `05-` resolutions as each lands; final commit/push per session-close
   protocol.

Steps 2 and 3 share artifact staging (see `01-current-state.md` §Staging), so
one lab session covering both is the efficient path.

## Exit Criteria (mirrors the request's acceptance criteria)

- [ ] Handoff evidence covering **every element** of both bead contracts, each
      with a citation (code + test + evidence file) or a landed gap-fix;
      Intel-VM-lane availability explicitly confirmed; filed to guest-sdk
      (bead notes + handback file) and acknowledged (bead state change or
      handback note).
- [ ] Retuned caps carry measurement-derived values with derivation comments;
      fixture-profile tests pass unchanged; real-emulator test's normal
      outcome is `BUDGET_REACHED` (not `HARD_CAP`); each cap ≤4× of
      (measured per-frame cost × frames it covers).
- [ ] `linux_m5` frame-scheduling gate green against a real-emulator image;
      timestamped evidence dir under `target/` recording this repo's rev +
      workload image identity.
- [ ] Wall-clock question closed empirically: either documented no-hang repro
      attempt → resolution note citing the harness, or hang reproduced →
      backstop implemented per the determinism constraints in `04-`.
- [ ] Resolution files present in
      `requests/phase3-snapshot-restore-no-frame-under-no-tick-take-two/`
      (closes `08-followup-frame-hard-cap.md`),
      `requests/nextsdkevent-run-wallclock-backstop/`, and
      `requests/phase3-frame-cap-retune-and-run-wallclock-backstop/04-resolution.md`.
- [ ] Beads created/closed for each work item; `bd dolt push` + `git push`
      done (CLAUDE.md session-close protocol).

## Out of Scope (from the request — do not fold in)

- Play-60fps M4 epoch-hash pipeline (bead `38b6`) — deferred pending emulator
  speedup.
- READY-snapshot regeneration (reference-workload `refwork-gp9` territory).
- Worker orphan-slot hardening (`determinism-hypervisor-umay`, bridge `72o`).
