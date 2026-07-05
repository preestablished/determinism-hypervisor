# Evidence

Clean-room: this file records refs, icounts, paths, and stop reasons only —
no ROM bytes, framebuffer pixels, WRAM contents, or operator secrets.

Sender evidence run: rom-operator-bridge @ `main` (bead rom-operator-bridge-9xo).
Proto: `determinism.hypervisor.v1` — `RunRequest` / `RunResponse`
(`reason`, `frames_elapsed`; `StopReason` values `HARD_CAP` / `BUDGET_REACHED`).

## The contrast — mind the axis

| Scenario | Harness | Boot vs restore | Result |
|---|---|---|---|
| Real workload, no-timer | guest-sdk `VmHarness` | **fresh boot** | **frames** — `f1 > f0`, 2 passed / 7.64s |
| Real/fixture workload, no-timer | guest-sdk `VmHarness` | **restore** (`from_snapshot`) | **frames** (guest-sdk resolution: `no_timer_ready_snapshot_restore_produces_next_frame` passes) |
| Real workload, no-timer | deployed **`dh-workerd`** | **restore** | **silent** — 0 frames, 0 drained GuestEvents / `10e9` instructions |
| Real workload, no-timer | in-process **dh-worker** (`linux_m5` test) | **fresh boot** | **silent** — `Run{frame_budget}` stops `HARD_CAP` (reason 4), 0 frames (see below) |

The two **dh-worker** cells fail; the two **VmHarness** cells pass. The failing
variable is the **harness** axis (VmHarness → dh-worker/dh-vmm), **on the boot
path** — not boot-vs-restore. Restore is not implicated as special.

## Failing case — deployed `dh-workerd`, restore then run

Snapshot ref (BLAKE3 content hash, non-secret):
`1499c0a77e883bc0f74d97a254e59508ea86b4d17976eba8cbf78e0c7961a270`
Regenerated at Ready by the M9 handoff (`NextSdkEvent(Ready)` stop, then
`TakeSnapshot`); its own `RestoreSnapshot verification succeeded: yes`.

Restore succeeds and reports the Ready boundary (restored slot icount
`643,201,618`). Then:

1. `Run{ lease, frame_budget = 1, hard_icount_cap = 0 }`
   → `reason = HARD_CAP`, `icount = 10,643,201,618`
   (= Ready icount `643,201,618` + worker-default cap `10e9`; the
   `0 ⇒ 10e9` default is applied in `dh-worker/src/service.rs` `hard_icount_cap`
   and `dh-vmm/src/runctl.rs`), `frames_elapsed = 0`, **55.2 s wall**.
   The run executed a full 10-billion-instruction budget and never hit the
   pv-pad `FRAME_COUNTER` MMIO frame-boundary exit.

2. Five successive `Run{ next_sdk_event = {}, hard_icount_cap = 800e6 }` steps
   → every one returns `reason = HARD_CAP` with **no `sdk_event`**:
   ```
   step 1: HARD_CAP icount=1,443,201,618   (no event)
   step 2: HARD_CAP icount=2,243,201,618   (no event)
   step 3: HARD_CAP icount=3,043,201,618   (no event)
   step 4: HARD_CAP icount=3,843,201,618   (no event)
   step 5: HARD_CAP icount=4,643,201,618   (no event)
   ```
   `next_sdk_event` with an unset stream filter matches **any** event
   (`dh-worker/src/service.rs` `sdk_event_matches`: `filter.is_none_or(...)`),
   so five HARD_CAP returns with no `sdk_event` mean **nothing was drained**.
   The restored guest is *executing* (icount advances ~4e9), not HLT-idle, but
   emits no `FrameMark`, `LogLine`, or region activity. (This proves "no event
   reached a drain/exit," which is *consistent with* — not proof of — the
   ring-W spin in `02-hypothesis.md`; a guest parked before its first emit
   would look identical.)

This reproduces with the bridge out of the loop; it is exactly the RPC
sequence `RealBackend::resume` issues
(`rom-operator-bridge/service/src/backend.rs:2214-2223`:
`Run{ hard_icount_cap: 0, until: FrameBudget(1) }`).

## Deciding control — dh-worker FRESH BOOT (no snapshot) also HARD_CAPs

Run **your own** `#[ignore]`d worker test against the real artifacts. Its first
`Run{frame_budget}` is on a freshly-`CreateVm`'d VM run to `Ready` — no
snapshot, no restore:

```
DH_M9_GUEST=linux \
DH_M9_BZIMAGE=<real reference-workload bzImage> \
DH_M9_INITRAMFS=<real reference-workload initramfs.cpio (decompressed)> \
DH_M9_BASE_IMAGE=<real base.img>  DH_M9_GAME_IMAGE=<real game.img> \
DH_M9_IMAGE_CACHE=<empty writable dir> \
cargo test -p dh-worker --test m5_frame_scheduling \
  linux_m5_frame_budget_records_post_ready_frame_marks -- --ignored --nocapture
```
```
running 1 test
test linux_m5_frame_budget_records_post_ready_frame_marks ...
  Error: "first Linux run stopped with reason 4, expected BudgetReached"
FAILED
```

The failure is at `m5_frame_scheduling.rs:82`
(`assert_worker_frame_budget(&first_run, FIRST_FRAMES, "first Linux run")`) —
**before** the snapshot/restore arm is ever reached. `m9_linux_ready_snapshot`
completed (so boot-to-`Ready` through dh-worker succeeded), and then the
fresh-boot `Run{frame_budget}` stopped `HARD_CAP` (StopReason 4;
`proto/hypervisor.proto:244`) with no frame — **not** `GUEST_HALTED` (6) or
`FAULTED` (7), so the guest ran to the cap rather than dying. The config is the
worker's own `m9_linux_machine_config` (forced no-tick cmdline). This is the
same no-frame symptom as the deployed restore, reproduced on a fresh boot
inside your own test harness. **It is the ready-made red repro for the fix.**

## Passing control — fresh boot, guest-sdk `VmHarness`, real artifact

```
DETGUEST_VM_TESTS=1 \
REFWORK_READY_INITRAMFS=<decompressed real reference-workload initramfs.cpio> \
REFWORK_READY_BZIMAGE=<real reference-workload bzImage> \
cargo test -p detguest-vmtest --test refwork_ready_hold -- --nocapture --test-threads=1
```
```
test no_timer_real_harness_reaches_and_holds_ready ... ok
test real_harness_reaches_and_holds_ready ... ok
test result: ok. 2 passed; 0 failed; finished in 7.64s
```
The no-timer arm sets `cfg.timer_interrupts = false` and
`cfg.cmdline = cfg.timerless_cmdline()`, boots to Ready, then asserts the meta
region frame counter advances past the first frame boundary
(`refwork_ready_hold.rs`: `f0` captured at line 141, `assert!(f1 > f0)` at
152-156). It passes — the real workload frames with no tick **on a fresh boot
through VmHarness**.

## Cmdline equivalence (the comparison is apples-to-apples)

- guest-sdk `TIMERLESS_CMDLINE_FLAGS` (`tests/vm/src/harness/mod.rs:52-53`):
  `notsc tsc=unstable clocksource=jiffies noapictimer lpj=4096`
- deployed worker `BZIMAGE_FORCED_CMDLINE` (`crates/dh-vmm/src/config.rs:92`)
  contains exactly those load-bearing no-tick flags:
  `... notsc tsc=unstable clocksource=jiffies vdso=0 lpj=4096 noapictimer ...`
  (the worker cmdline has additional non-timer flags — `nokaslr`, `vdso=0`,
  hugepages, etc.; the equivalence claim is scoped to the load-bearing no-tick
  flags, which match exactly).

The guest-sdk harness comment (`harness/mod.rs:43-44`) cites
`crates/dh-vmm/src/config.rs:92` as the source of these flags. So the
fresh-boot "frames advance" result holds under the deployed no-tick flags, and
the dh-workerd failure is not explained by any cmdline difference.

## Repro artifacts

- The fresh-boot control uses `reference-workload/dist/workload-image-0.1.0/`
  (`bzImage`, `initramfs.cpio.zst`). **Note:** `dist/` is `.gitignore`d
  (`reference-workload/.gitignore:5`) — these are **not** obtainable by pulling
  the repo. To regenerate: `cargo xtask image build` in `reference-workload`
  (double-build via `image double-build`), then
  `zstd -d dist/workload-image-0.1.0/initramfs.cpio.zst`. Or use the guest-sdk
  M4 frame-loop fixture as the in-repo stand-in (see `03-ask.md` §1).
- Game delivered via the pv-blk path (`/dev/vdb`, `game_source = "pv-blk"`,
  the reference-workload demo game). `refwork_ready_hold` frames with a NOP
  ROM under VmHarness, so ROM content is *unlikely* to be implicated — but this
  was **not** tested under dh-workerd; fold the real game into the §1 control.
- The Ready snapshot is regeneratable end-to-end via the M9 handoff; the
  deployed instance is ref `1499c0a7…` above.
