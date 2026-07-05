# The ask

## 1. H1 vs H2 is already decided — it is H2 (we ran your own test)

We ran the deciding control so you don't have to: your `#[ignore]`d
`linux_m5_frame_budget_records_post_ready_frame_marks` against the real
artifacts. Its **first, fresh-boot** `Run{frame_budget}` (no snapshot) stops
`HARD_CAP` with no frame — it fails at `m5_frame_scheduling.rs:82` before the
restore arm (command + output in `01-evidence.md`). So this is **H2**: the
dh-worker/dh-vmm **no-tick `Run` frame/drain path fails on the boot path
itself**; restore is not special. You already have the red repro — it's your
own test.

## 2. Fix the no-tick `Run` ring-W frame/drain path; make `linux_m5` a CI gate (primary deliverable)

The observable to fix: a workload that reaches `Ready` through dh-worker under
`BZIMAGE_FORCED_CMDLINE` must reach its first pv-pad `FRAME_COUNTER` when run
with `frame_budget`, i.e. `Run{frame_budget=N}` stops `BUDGET_REACHED` with
`frames_elapsed == N` — **not** `HARD_CAP` — on a **fresh boot** (and,
consequently, after restore).

Make `linux_m5_frame_budget_records_post_ready_frame_marks` a CI gate rather
than `#[ignore]`d — either by staging the DH_M9_* artifacts in CI, or by
adding a sibling test that drives the same
boot → `Run{frame_budget}` → assert-`BUDGET_REACHED` path with a fixture whose
frames go through the real `frame_mark()` → ring-W `EventClass::Critical` emit
(**not** `nanokernel::fake_frames_elf()`, which writes `FRAME_COUNTER` directly
and bypasses the ring-W doorbell/drain — the very path under suspicion). The
existing non-ignored `m5_accept_frame_budget_and_at_frame_absolute_across_restore`
(line 384) uses that fake-frames kernel, which is why CI is green today while
the real no-tick path is broken.

## 3. Secondary — the restore-time DetChannelDevice re-attach

Once the boot path frames, re-check restore: does `restore_engine.rs`'s device
restore + the DetChannelDevice EVTC re-attach (`dh-devices/.../detchannel.rs`)
re-establish the same host-side ring-W drain / doorbell servicing (also check
`restore_producer_seqs` for a ring-W producer-sequence gap)? This is only a
concern if restore *additionally* breaks servicing on top of the boot-path
fix — verify it with the restore arm of `linux_m5` after §2 lands.

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

- `linux_m5_frame_budget_records_post_ready_frame_marks` (against the real
  artifacts) currently fails at its **first, fresh-boot** `Run{frame_budget}`
  (`HARD_CAP`, 0 frames) — the confirmed red state — and passes end to end
  after the fix.
- A **freshly-booted** Ready workload (and, consequently, a restored one), run
  with `frame_budget = N` under the timerless cmdline, stops at
  `BUDGET_REACHED` with `frames_elapsed == N` — verified in CI without staged
  private artifacts (fixture-driven if needed, through the real `frame_mark()`
  ring-W path).
- No determinism regression: the fix must not perturb the DHILOG, the state
  hash, or the replay paths (ARCH §8.3, §6.10 C5).

## Handback

Please record a resolution (your convention) with the H1/H2 outcome, the test
name, the root cause, and the fix commit. We will re-run the deployed
`RestoreSnapshot → Run(frame_budget=1)` against ref `1499c0a7…` (or a freshly
regenerated Ready snapshot) and confirm the browser renders the first real
frame. This is the last gate for Phase 3 workload-in-the-box.
Tracking bead: rom-operator-bridge-9xo.

## Reproduction handles (non-secret)

- **Red repro (in your own tree):** `linux_m5_frame_budget_records_post_ready_frame_marks`
  with `DH_M9_*` env pointed at the real reference-workload artifacts — fails at
  the fresh-boot `Run{frame_budget}` (`HARD_CAP`). Exact command + output in
  `01-evidence.md`. Build the artifacts via `cargo xtask image build` in
  `reference-workload` (`dist/` is git-ignored; decompress `initramfs.cpio.zst`).
- Passing control (already green): guest-sdk `refwork_ready_hold` no-timer arm
  with `REFWORK_READY_INITRAMFS` / `REFWORK_READY_BZIMAGE` — same workload, same
  no-tick flags, frames in VmHarness.
- Deployed corroboration: `dh-workerd` `RestoreSnapshot` of Ready snapshot ref
  `1499c0a77e883bc0f74d97a254e59508ea86b4d17976eba8cbf78e0c7961a270` then
  `Run{ frame_budget = 1, hard_icount_cap = 0 }` — `HARD_CAP` at `+10e9` icount,
  `frames_elapsed = 0`, ~55 s wall.
- Proto: `determinism.hypervisor.v1.RunRequest` / `RunResponse`
  (`reason`, `frames_elapsed`); `StopReason` `HARD_CAP = 4`
  (`proto/hypervisor.proto:244`).
