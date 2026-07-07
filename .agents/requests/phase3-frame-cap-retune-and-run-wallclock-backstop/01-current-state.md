# Current State (Evidence-Based)

Repo `main` at `bdd476b` ("Merge play-60fps M1-M3", 2026-07-07), assessed
against the phase plan and this repo's own request/plan trail.

## What Is Done (Not Re-Litigated Here)

- **M0–M9 accepted.** Phase 1/2 exit gates documented
  (`docs/phase-1-exit-gate.md`, `docs/phase-2-exit-gate.md`); M9
  Linux-guest final acceptance with Linux re-runs of the M4/M5/M7 gates at
  `target/m9-final-acceptance-20260621T004402Z/`.
- **Capture engine + framebuffer contract.** D7 `layout_version 1`
  geometry (`5698d7e`, `docs/decisions/framebuffer-region-geometry.md`);
  resolved and deploy-verified in
  `requests/rom-bridge-getframebuffer-region-contract/`.
- **Play-60fps plan M1–M3 merged** at `bdd476b` (`RunWithFrameCapture`
  streaming RPC, frame-hold input injection; beads `0xzb`, `38ex`, `7kxy`
  closed; dual review applied at `cab9fa4`). M4 (epoch-hash pipeline, bead
  `38b6`) is measured-and-deferred: the bead's 2026-07-07 note records
  ~27.8M instr/frame, making 60fps unreachable without a
  reference-workload emulator speedup — correctly not this repo's problem.

## Open Item 1: The Frame Caps Are Fixture-Calibrated

Trail: `requests/phase3-snapshot-restore-no-frame-under-no-tick/` (resolved
as non-repro, then **reopened** by the bridge's `09-verification.md` — the
non-repro had used a stale synthetic initramfs) → the take-two dir, whose
`07-handoff-resolution.md` localized the real cause to reference-workload's
harness running `NoopPlatform` (no `frame_mark`), filed `refwork-4qj`,
**fixed** at reference-workload `40eaf4f`.

The surviving follow-up is `08-followup-frame-hard-cap.md`: with the
refwork fix in, the real-emulator path passes **except** that the test
caps were calibrated for the synthetic fixture:

- `FRAME_HARD_CAP = 50_000_000` at
  `crates/dh-worker/tests/m5_frame_scheduling.rs:47`;
- `DETCHANNEL_FRAME_HARD_CAP = 1_000_000` at line 48 of the same file —
  the follow-up asks that it be retuned too if it gates the real-emulator
  path;
- `LINUX_FRAME_HARD_CAP = 50_000_000` at `m5_net_loopback.rs:47` (same
  value, different identifier).

The real emulator measures ~25M instr/frame (take-two measurement,
2026-07-06; bead `38b6`'s newer figure is 27.8M), so a 3-frame budget
overruns the 50M cap. The follow-up's concrete recommendation is
`FRAME_HARD_CAP ≈ 150_000_000`. This is a test-tuning change in this
repo; nothing in production code is implicated.

## Open Item 2: Does Run Actually Hang? (Confirm, Then Maybe Backstop)

`requests/nextsdkevent-run-wallclock-backstop/` — filed 2026-07-05 by
rom-operator-bridge, from guest-sdk's `phase3-boot-scheduling-deadlock`
resolution action item #3. No resolution file exists.

The filing is deliberately titled confirm-first, and the code partly
answers it already: the run loop treats **terminal HLT as a stop**
(`crates/dh-vmm/src/runctl.rs` — `StopReason::GuestHalted`). The genuinely
open failure shapes are (a) an *idle* HLT (IF=1, guest waiting for a tick
that never comes under no-tick) blocking inside `KVM_RUN` without
returning, and (b) a non-HLT block that retires no instructions — in
either, the icount HARD_CAP never trips and `Run{until: NextSdkEvent}`
would not return. The interim stopgap in the filing was wrapping the
client call in `timeout(1)`; the motivating run itself reached READY fine,
so **no live hang has actually been observed** — which is exactly why the
filing says "if HLT handling already covers it, close this with a note."

Determinism note (what any implementation must preserve): wall-clock must
never influence guest-visible execution. A deadline may only abort a Run
*from the host side*, with the guest state discarded or left restorable —
it must not inject a guest-visible event at a nondeterministic instruction
count, and a backstop-aborted run must never be committed as replayable
evidence (its input log is truncated at a nondeterministic point).

## Open Item 3: The Unissued Handoff Gating Phase 3 Exit Gate 2

guest-sdk's Ms5 `determinism_replay` CI gate — named directly in Phase 3
exit gate 2 (`phase-3-workload-in-the-box.md`) — is blocked by two P0
beads in *their* tracker that point at *this* repo:

- `guest-sdk-ext-hyp-input-log-dev-events` — PAD_SET / DEV_EVENT encodings
  (ring C/I pushes, ring A/W consumer bumps, `pio_answer`);
- `guest-sdk-ext-hyp-determinism-replay-linux` — bit-identical Linux
  replay gate, replay-mode input-log application.

Both were last updated **2026-06-18**, three days *before* the M9
acceptance. The capabilities appear to exist now: `crates/dh-inputlog/src/dhilog.rs`
defines `KIND_PAD_SET`/`KIND_DEV_EVENT` including `pio_answer`;
`crates/dh-worker/src/replay_engine.rs` applies `PadSet`/`DevEvent` in
replay mode; the M9 evidence includes the Linux M5 record-replay corpus
gate (`17-linux-m5-corpus.log`). What does not exist is a verification of
coverage against each element of the two bead contracts — including the
ring A/W consumer-bump encodings, which no quick grep confirms — **on the
Intel VM lane guest-sdk's beads require**, communicated back so guest-sdk
can unblock. The beads' own unblock note: "when the corresponding
hypervisor surface is shipped *and available to the Intel VM lane*."

## Who Is Waiting

- **Phase 3 exit gate 2**: guest-sdk Ms3 input acceptance and the Ms5
  `determinism_replay` gate — blocked on item 3's handoff.
- **Phase 3 exit gate 3** (first room in-VM): reference-workload's
  `refwork-gp9` image/READY-snapshot regeneration is the other half; when
  it lands, this repo's `linux_m5`-against-real-image gate must be green
  to call the milestone durable rather than once-lucky.
- **Phase 5 orchestrator** (unattended thousands of Runs): if the idle-HLT
  hang is real, the backstop is the difference between a retryable error
  and a wedged slot during a 4-hour soak (`phase-5-closed-loop.md`
  gate 5).
