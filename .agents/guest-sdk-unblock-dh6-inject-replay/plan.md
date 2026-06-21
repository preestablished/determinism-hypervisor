# Guest SDK Unblock DH-6 Inject Replay Plan

## Source Plan

Parent plan:
`/home/infra-admin/git/preestablished/.agents/plans/guest-sdk-unblock/determinism-hypervisor/plan.md`

## Verdict

The parent plan is mostly implemented in this repo, but not fully complete for
guest-sdk Milestone 5 / DH-6.

Treat these as already implemented and do not rework them unless a regression is
found:

- DH-0: Phase 2 / M7 floor has recorded acceptance evidence in
  `docs/phase-2-exit-gate.md`, including 1000/1000 Linux fork VerifyReplay.
- DH-1: Linux bzImage + initramfs boot to guest-sdk Ready EventKind 14 is
  implemented and documented in `docs/phase-1-exit-gate.md` and
  `docs/phase-2-exit-gate.md`.
- DH-2: Linux pv-pad frame scheduling is implemented by `frame_budget`,
  `at_frame`, FRAME_MARK handling, and the Linux M5 frame scheduling gate.
- DH-3: detchannel host mutation logging exists through `ChannelWriteSink` and
  canonical DEV_EVENT records in `crates/dh-devices/src/detchannel.rs`.
- DH-4: `ReadGuestMemory(region=...)` is implemented in
  `crates/dh-worker/src/service.rs` and resolves names through the detchannel
  manifest with layout-version checks.
- DH-5: `CaptureSpec` on `Run` and `TakeSnapshot` is implemented with
  manifest resolution, packed `feature_bytes`, `fb_lz4`, and layout-version
  errors.

The remaining work is DH-6 completion:

- Replay must answer guest-sdk `inject_point` / `IN 0xD384` from recorded
  PIO_ANSWER values with the synthesizer absent. Current replay uses the
  default detchannel fault plan and normalizes generated PIO_ANSWER records; it
  does not prove non-zero recorded inject decisions are the source of replay
  answers.
- VerifyReplay divergence reporting must distinguish skipped input, channel
  mutation drift, and PIO answer mismatch in `suspected_cause`.

## Relevant Existing Code

- `crates/dh-devices/src/detchannel.rs`
  - `DetChannelHost::pio_in` logs every detcall IN answer as
    DEV_EVENT/PIO_ANSWER.
  - `DetChannelHost::inject_answer` delegates `PORT_INJECT` answers to
    `InjectResponder<P: FaultPlan>`.
  - `CtxSink` implements `ChannelWriteSink` and logs RING_PUSH, CONS_BUMP, and
    PIO_ANSWER records.
  - Unit test `inject_flow_answers_via_plan_and_logs_once` proves non-zero
    TableFaultPlan recording at device level.
- `crates/dh-worker/src/replay_engine.rs`
  - `ReplayDetChannel` currently uses `detguest_host::LogFaultPlan`.
  - `detchannel_exit_generated_event` treats EVENT_PIO_ANSWER and
    EVENT_CONS_BUMP as generated detchannel outputs.
  - Canonical DEV_EVENT records that will regenerate are skipped during replay
    application, so replay correctness depends on the replay detchannel fault
    plan producing the same PIO_ANSWER value.
- `crates/dh-worker/src/service.rs`
  - Runtime detchannel devices are created with `LogFaultPlan::default()`.
  - VerifyReplay maps bisection divergences into the string
    `suspected_cause`, but current causes are generic hash/comparison strings.
- `proto/hypervisor.proto`
  - `Divergence.suspected_cause` is the only public field needed for distinct
    cause labels; a proto change should not be required unless a structured
    enum is explicitly desired.

## Implementation Plan

### 1. Preserve the completed M9 surface

Before changing replay code, run or at least compile the narrow existing tests
that protect already-complete surfaces:

- `cargo test -p dh-devices detchannel`
- `cargo test -p dh-worker --test replay_engine`
- `cargo test -p dh-worker --lib service::tests::verify_replay`

On a KVM host with M9 artifacts, keep these as acceptance gates:

- `DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test linux_worker_api --release -- --ignored --nocapture`
- `DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test m5_record_replay --release linux_m5_record_replay_post_ready_corpus_reverifies -- --ignored --nocapture`

### 2. Add a log-backed replay fault plan

Implement a local replay fault plan in `dh-worker`, or seed the existing
`detguest_host::LogFaultPlan` if it already has the needed constructor in the
sibling guest-sdk dependency.

Required behavior:

- Build the plan from the replay input log before execution starts.
- Extract canonical DEV_EVENT/PIO_ANSWER records for `PORT_INJECT` (`0xD384`).
- Preserve record order and enough boundary identity to reject ambiguous or
  missing answers. The preferred key is `(icount, port, occurrence_index)`;
  include sequence number in diagnostics.
- Implement `FaultPlan` so `InjectResponder::answer` returns the recorded
  packed `FaultDecision` for the matching replayed inject query.
- Do not consult input-synthesizer state, wall time, randomness, or a table
  plan on replay.
- If replay reaches an inject query with no matching recorded answer, fail
  verification with `suspected_cause` containing `pio_answer_missing`.
- If the replayed inject query order or port/value conflicts with the log,
  fail verification with `suspected_cause` containing `pio_answer_mismatch`.

Expected code locations:

- Add a small module or private type near `crates/dh-worker/src/replay_engine.rs`.
- Change the replay detchannel type/factory to use the log-backed plan for
  replay slots.
- Keep the runtime/service recording path on the recording fault plan path; do
  not weaken normal exploration behavior to make replay pass.

### 3. Prove non-zero PIO_ANSWER replay

Add a worker-level test that records and replays at least one non-zero
`FaultDecision`.

Minimum host-runnable shape:

- Use the existing detchannel channel-page test helpers or a small KVM fixture
  that emits an `InjectQuery`.
- Record with `TableFaultPlan`, for example
  `FaultDecision::Platform { kind: 2, arg: 512 }`.
- Assert the sealed DHILOG contains a DEV_EVENT/PIO_ANSWER for `PORT_INJECT`
  with packed value `0x0002_0002`.
- Replay with no synthesizer/table plan present.
- Assert the replayed guest-visible result and resealed replay log use the
  recorded value, not `Proceed`.

Linux/GS-7 acceptance shape once the guest fixture is available:

- Boot the Linux guest to Ready.
- Run a workload segment that emits multiple guest-sdk `inject_point` calls.
- Record with a non-trivial fault plan.
- VerifyReplay from the root snapshot using only the DHILOG.
- Assert no Divergence and assert the workload-observed decisions match the
  recorded PIO_ANSWER values.

### 4. Add distinct divergence attribution

Keep the public proto stable by using stable `suspected_cause` string prefixes.

Required prefixes:

- `skipped_input`: replay reached a boundary where a canonical input record was
  expected but absent, late, or impossible to land.
- `channel_mutation_drift`: replay output differs in detchannel RING_PUSH or
  CONS_BUMP payload/order.
- `pio_answer_mismatch`: replay output differs in detchannel PIO_ANSWER port or
  value.
- `pio_answer_missing`: replay encountered an inject IN with no logged answer.

Implementation notes:

- Classify differences as close to the comparison point as possible, before
  falling back to generic `EPOCH_HASH` or `end_state_hash` text.
- Reuse existing `ReplayError::Divergence` / `BisectionDivergence` plumbing
  unless a local error variant would make the mapping cleaner.
- Include the relevant icount, record sequence, device id, event type, and port
  in the cause string where available.

### 5. Tests for attribution

Add focused mutation tests around VerifyReplay:

- Delete or move a canonical PAD_SET/DEV_EVENT record and assert
  `suspected_cause` contains `skipped_input`.
- Mutate a detchannel RING_PUSH or CONS_BUMP DEV_EVENT payload and assert
  `suspected_cause` contains `channel_mutation_drift`.
- Mutate a `PORT_INJECT` PIO_ANSWER value and assert `suspected_cause`
  contains `pio_answer_mismatch`.
- Omit the recorded PIO_ANSWER for an inject query and assert
  `suspected_cause` contains `pio_answer_missing`.

Prefer host-runnable unit/integration tests for classification. Add ignored
Linux/KVM coverage only for the end-to-end GS-7 path.

## Acceptance Criteria

- Existing Linux M9/M4/M5/M7 gates remain unchanged; this plan does not relax
  the already recorded M9 evidence.
- Replay of an inject-bearing guest segment succeeds with no synthesizer and a
  non-zero recorded `FaultDecision`.
- A replay run cannot silently substitute `Proceed` for a recorded non-zero
  PIO_ANSWER.
- VerifyReplay reports distinct `suspected_cause` prefixes for skipped input,
  channel mutation drift, PIO answer mismatch, and missing PIO answer.
- New host-runnable tests cover log-backed replay fault-plan behavior and
  attribution mapping.
- A Linux/GS-7 ignored test or documented command is added once the sibling
  guest-sdk fixture emits real `inject_point` traffic.

## Out Of Scope

- Rebuilding Linux READY, frame scheduling, ReadGuestMemory, CaptureSpec, or
  M7 fork/VerifyReplay.
- Implementing state scoring or feature-map parsing.
- Editing sibling `guest-sdk` unless the existing `detguest-host` API makes a
  log-backed replay fault plan impossible in this repo; if that happens, file a
  sibling issue and keep this repo change blocked on that API.
