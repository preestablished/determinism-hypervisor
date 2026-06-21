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
  mutation drift, and PIO answer mismatch in `suspected_cause`, as streamed
  Divergence evidence rather than as generic `DATA_LOSS` / infrastructure
  errors.

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
  - `FaultPlan::decide` does not receive icount or port and cannot return an
    error; missing/mismatched replay answers need an explicit diagnostic path.
- `crates/dh-worker/src/service.rs`
  - Runtime detchannel devices are created with `LogFaultPlan::default()`.
  - Restore recreates the detchannel responder with a fresh plan factory, so
    both the initial replay bus and restore path must use the same parsed
    DHILOG-backed plan source.
  - VerifyReplay maps bisection divergences into the string
    `suspected_cause`, but current causes are generic hash/comparison strings.
- `/home/infra-admin/git/preestablished/guest-sdk/crates/detguest-host/src/inject.rs`
  - `LogFaultPlan` is currently a Proceed-only replay skeleton. If the needed
    replay cursor cannot be built cleanly from this repo, update or file the
    required sibling `guest-sdk` issue first.
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

- Prerequisites: `DH_M9_BZIMAGE`, `DH_M9_INITRAMFS`, `DH_M9_BASE_IMAGE`,
  `DH_M9_GAME_IMAGE`, `DH_M9_IMAGE_CACHE`, `DH_M9_ALLOW_SKIP=0`, KVM with
  dirty-ring support, reserved slot cores for M7, and `taskset` using those
  cores.
- `DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test linux_worker_api --release -- --ignored --nocapture`
- `DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test m5_record_replay --release linux_m5_record_replay_post_ready_corpus_reverifies -- --ignored --nocapture`
- Regression-only M7 preservation gate:
  `DH_M9_ALLOW_SKIP=0 DH_M7_ACCEPT_GUEST=linux DH_M7_ACCEPT_JOBS=1000 DH_M7_CROSS_CHECKS=10 DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_ACCEPT_ALLOW_SKIP=0 taskset -c 2-5 cargo test -p dh-worker --test m7_fork_verify --release -- --ignored --nocapture --test-threads=1`
- Regression-only M7 cross-slot gate:
  `DH_M9_ALLOW_SKIP=0 DH_M7_ACCEPT_GUEST=linux DH_M7_ACCEPT_JOBS=1000 DH_M7_CROSS_CHECKS=10 DH_M7_ACCEPT_SLOT_CORES=2-5 DH_M7_ACCEPT_ALLOW_SKIP=0 taskset -c 2-5 cargo test -p dh-worker --test m7_fork_verify --release m7_accept_cross_slot_rerun_10_seeded_forks_identical_refs -- --ignored --nocapture`

### 2. Add a log-backed replay fault plan

Implement a replay answer source in `dh-worker`, or extend/seed the existing
`detguest_host::LogFaultPlan` in the sibling guest-sdk dependency if that is the
right ownership boundary. Today that type is a skeleton that always returns
`Proceed`, so do not assume it satisfies this plan without checking the actual
dependency version.

Required behavior:

- Build the plan from the replay input log before execution starts.
- Extract canonical DEV_EVENT/PIO_ANSWER records for `PORT_INJECT` (`0xD384`).
- Preserve strict record order. Because `FaultPlan::decide` only receives
  `(iseq, name_id, name)`, use icount, port, DHILOG sequence number, and
  occurrence index as diagnostics/cursor validation, not as direct
  `FaultPlan` inputs, unless the guest-sdk API is deliberately changed.
- Implement or seed `FaultPlan` so `InjectResponder::answer` returns the
  recorded packed `FaultDecision` for the matching replayed inject query.
- Do not consult input-synthesizer state, wall time, randomness, or a table
  plan on replay.
- Since `FaultPlan::decide` is infallible, expose replay cursor/error state
  outside the plain return value and check it immediately after detchannel IN
  handling. Alternatively, make an intentional guest-sdk API change to a
  fallible responder/plan and update both repos together.
- If replay reaches an inject query with no matching recorded answer, convert
  that to a streamed VerifyReplay Divergence with `suspected_cause` containing
  `pio_answer_missing`; do not let it escape as `Apply`, `Run`, or generic
  `DATA_LOSS`.
- If the replayed inject query order, port, or value conflicts with the log,
  convert that to a streamed VerifyReplay Divergence with `suspected_cause`
  containing `pio_answer_mismatch`.

Expected code locations:

- Add a small module or private type near `crates/dh-worker/src/replay_engine.rs`.
- Change the replay detchannel type/factory to use the log-backed plan for
  replay slots.
- Seed both the initial replay bus and the detchannel restore-plan factory from
  the parsed DHILOG. Restore currently installs a fresh plan; a one-time seed at
  VM construction is not enough.
- Keep the runtime/service recording path on the recording fault plan path; do
  not weaken normal exploration behavior to make replay pass.

### 3. Prove non-zero PIO_ANSWER replay

Add a worker-level replay or VerifyReplay test that records and replays at least
one non-zero `FaultDecision`. The test must exercise the `ReplayDetChannel`
path and fail on the current Proceed-only `LogFaultPlan`; do not settle for
another device-local `DetChannelHost` unit test, because that is already covered.

Minimum host-runnable shape:

- Use a synthetic/test-only DHILOG if the production worker cannot yet inject a
  non-trivial fault plan through the service API. Production non-zero recording
  still needs a synthesizer/fault-plan injection path.
- The fixture must include an `InjectQuery` and a DEV_EVENT/PIO_ANSWER for
  `PORT_INJECT` with a non-zero packed value, for example
  `FaultDecision::Platform { kind: 2, arg: 512 }` packed as `0x0002_0002`.
- Replay with no synthesizer/table plan present, through the worker replay path.
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
- `channel_mutation_drift`: replay-applied or generated detchannel mutation
  records differ in payload/order after the implementation has a real comparison
  point for that record class.
- `pio_answer_mismatch`: replay output differs in detchannel PIO_ANSWER port or
  value.
- `pio_answer_missing`: replay encountered an inject IN with no logged answer.

Generated detchannel diffing:

- Current replay treats generated detchannel CONS_BUMP and PIO_ANSWER records
  specially; classify first normalized-record mismatches for these before
  falling back to generic reseal divergence.
- RING_PUSH is not currently in the generated-output set. Either implement
  replay application/comparison for RING_PUSH channel mutations before using
  `channel_mutation_drift` for it, or keep RING_PUSH out of that label.
- Be explicit about whether a record is "generated by replay and compared" or
  "canonical input applied by replay"; the diagnostic label should match that
  path.

Implementation notes:

- Classify differences as close to the comparison point as possible, before
  falling back to generic `EPOCH_HASH` or `end_state_hash` text.
- Add typed replay divergence causes before `Run`/`Apply` errors escape into
  gRPC status mapping. Reuse existing `ReplayError::Divergence` /
  `BisectionDivergence` plumbing unless a local error variant makes the mapping
  cleaner.
- Include the relevant icount, record sequence, device id, event type, and port
  in the cause string where available.

### 5. Tests for attribution

Add focused mutation tests around VerifyReplay:

- Delete or move a canonical PAD_SET/DEV_EVENT record and assert
  `suspected_cause` contains `skipped_input`.
- Mutate a generated detchannel CONS_BUMP payload/order and assert
  `suspected_cause` contains `channel_mutation_drift`, if the generated-output
  comparison now supports that path.
- Add a RING_PUSH mutation test only after replay actually applies or compares
  RING_PUSH channel memory effects; otherwise document it as a blocked follow-up.
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
- Missing or mismatched replay PIO answers stream VerifyReplay Divergence
  records with stable `suspected_cause` prefixes instead of surfacing as
  generic gRPC failures.
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
