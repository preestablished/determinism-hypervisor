# The ask

## 1. Decide H1 vs H2 first: a dh-workerd fresh-boot frame control

Before fixing anything, run the one experiment that tells us whether this is
restore-specific (**H1**) or a general dh-workerd no-tick drain failure
(**H2**): **fresh boot through `dh-workerd`** (no restore) of the real workload
(or a guest-sdk frame-loop fixture that reaches Ready and frames), under
`BZIMAGE_FORCED_CMDLINE`, then `Run{ frame_budget = 1 }`.

- If it stops `BUDGET_REACHED` with `frames_elapsed >= 1` → **H1**: the defect
  is in the restore path (re-attach). Proceed to §2.
- If it also returns `HARD_CAP` / 0 frames → **H2**: retarget the whole
  investigation at the dh-workerd no-tick `Run` ring-W drain path (boot and
  restore alike); §2's restore focus becomes secondary.

We (rom-operator-bridge) can run this control too if you'd rather we produce
it — say the word. It is the single result that makes the request's title
precise.

## 2. A regression test on the exact deployed path (primary deliverable)

Add a **dh-worker / dh-vmm** integration test, running in CI, that exercises
the path the deployed bridge uses under the forced timerless cmdline
(`BZIMAGE_FORCED_CMDLINE`, no-tick):

1. Boot the real reference-workload image **or** a guest-sdk frame-loop fixture
   that reaches Ready and emits a **ring-W** `FrameMark` via the real
   `frame_mark()` path (see below) to the guest-sdk `Ready` event.
2. `TakeSnapshot`.
3. `RestoreSnapshot` into a fresh slot.
4. `Run{ until: frame_budget = 1 }`.
5. **Assert** `reason == BUDGET_REACHED` and `frames_elapsed >= 1`, not
   `HARD_CAP`.

**Why the existing coverage does not catch this** (please don't close as
"already tested"): `crates/dh-worker/tests/m5_frame_scheduling.rs:50`
`linux_m5_frame_budget_records_post_ready_frame_marks` does boot → snapshot →
restore → frame_budget through the worker on an M9 Linux Ready snapshot, but it
is `#[ignore]`d (line 49: "requires KVM dirty-ring support and staged DH_M9_*
artifacts") so it does not run in CI; and the non-ignored
`m5_accept_frame_budget_and_at_frame_absolute_across_restore` (line 384) boots
`nanokernel::fake_frames_elf()` (line 394), which writes `FRAME_COUNTER`
**directly** and never exercises the guest-sdk **ring-W** `emit()`/doorbell
drain. So no *CI-run* test exercises post-restore ring-W frame emission — the
exact path suspected in `02-hypothesis.md`. The new test should either
un-`#[ignore]` the Linux path with staged artifacts, or use a fixture whose
frames go through `frame_mark()` → ring-W `EventClass::Critical` emit.

## 3. Investigate the restore-time DetChannelDevice re-attach (if H1)

Determine whether `restore_engine.rs`'s device restore + the DetChannelDevice
EVTC re-attach (`dh-devices/.../detchannel.rs`) re-establishes the **host-side
ring-W drain / doorbell servicing** that a fresh boot's device loop provides —
or whether it only re-seats the device's guest-memory handle and fault plan
(and check `restore_producer_seqs` for a ring-W producer-sequence gap). If the
drain servicing is not re-armed on restore, that is the fix: re-arm it so a
restored guest's `DOORBELL_RING_W` is answered exactly as on a fresh boot.

## 4. Confirm or refute the mechanism

The signature is **icount advances + zero GuestEvents drained**
(`02-hypothesis.md`). Please confirm whether, post-restore (and, per §1, on
fresh boot), the guest is spinning in a critical ring-W `frame_mark()` emit
(`EventClass::Critical` = "doorbell and retry until published") that the worker
never services. If the real cause is elsewhere (a device-state or vCPU-events
restore gap that parks the frame loop before its first emit, or a bad snapshot
boundary), that finding is just as valuable — the observable we need fixed is
"the workload reaches its first `FRAME_COUNTER` under no-tick through
dh-workerd."

## Acceptance

- The new CI test **reproduces the failure first** (`HARD_CAP`, 0 frames) on
  `main`, then passes after the fix. The red/green oracle is either the
  un-`#[ignore]`d Linux restore test with staged artifacts, or — if a
  ring-W fixture does not reproduce `HARD_CAP` — the sender re-running the
  deployed `RestoreSnapshot → Run(frame_budget=1)` against ref `1499c0a7…`.
- A restored (and, per §1, freshly-booted) Ready workload, run with
  `frame_budget = 1` under the timerless cmdline, stops at `BUDGET_REACHED`
  with `frames_elapsed >= 1`.
- No determinism regression: the fix must not perturb the DHILOG, the state
  hash, or the fresh-boot / replay paths (ARCH §8.3, §6.10 C5).

## Handback

Please record a resolution (your convention) with the H1/H2 outcome, the test
name, the root cause, and the fix commit. We will re-run the deployed
`RestoreSnapshot → Run(frame_budget=1)` against ref `1499c0a7…` (or a freshly
regenerated Ready snapshot) and confirm the browser renders the first real
frame. This is the last gate for Phase 3 workload-in-the-box.
Tracking bead: rom-operator-bridge-9xo.

## Reproduction handles (non-secret)

- Fresh-boot VmHarness control (already green): `refwork_ready_hold` with
  `REFWORK_READY_INITRAMFS` / `REFWORK_READY_BZIMAGE` pointed at a locally
  built `reference-workload/dist/workload-image-0.1.0/` (note: `dist/` is
  git-ignored — build via `cargo xtask image build`; see `01-evidence.md`).
- Failing path: deployed `dh-workerd` `RestoreSnapshot` of a Ready snapshot
  then `Run{ frame_budget = 1, hard_icount_cap = 0 }` — expect `HARD_CAP` at
  `+10e9` icount, `frames_elapsed = 0`, ~55 s wall.
- Deployed snapshot ref:
  `1499c0a77e883bc0f74d97a254e59508ea86b4d17976eba8cbf78e0c7961a270`.
- Proto: `determinism.hypervisor.v1.RunRequest` / `RunResponse`
  (`reason`, `frames_elapsed`).
