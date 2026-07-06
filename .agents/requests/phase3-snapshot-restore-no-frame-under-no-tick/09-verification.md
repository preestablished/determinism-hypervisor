# Verification (rom-operator-bridge, 2026-07-06)

## Verdict: reopen — the resolution does not yet cover the deployed workload

Branch `codex/determinism-hypervisor-tqvb-phase3-no-frame-restore` @ `891c5b6`
adds a genuinely useful detchannel ring-W drain regression
(`detchannel_frame_budget_drains_sdk_frame_marks_across_restore`) — please keep
it; unlike the old `fake_frames_elf` test it emits real ring-W `FrameMark`
records through the critical doorbell before writing `FRAME_COUNTER`, so it does
exercise the drain seam. And given the initramfs staged in the local M9 cache,
your PASS was the correct read and holding production code unchanged was the
right discipline — `04-current-state.md` was explicit that the M9 pass "does not,
by itself, prove that dh-worker drained detchannel FrameMark SDK events." The
gap is **artifact provenance**, not your analysis.

The problem: `08-resolution.md`'s non-repro ran against a **stale/drifted cache
artifact** — a synthetic guest-sdk fixture, not the real emulator that ships in
the deployed snapshot. With the real emulator the failure reproduces on this
exact branch, no production code changed. So this should not be closed as a
resolution of the reported bug.

## Root cause of the non-repro: fixture vs. real emulator

`08-resolution.md` / `04-current-state.md` ran the repro against the initramfs
staged in the local M9 cache. That is **not** the initramfs in the deployed
snapshot. They differ (only the initramfs differs; `bzImage`, `base.img`,
`game.img` are byte-identical across both sets):

| | Cache initramfs (resolution used) | Deployed initramfs (snapshot `1499c0a7`) |
|---|---|---|
| sha256 (first12) | `d15a9debf6d4` | `b6f5a7cdb1c0` |
| size | 1,156,096 B (Jun 20) | 1,500,404 B (Jul 5) |
| guest payload | `opt/m9-refwork-contract` (521,504 B) — a guest-sdk **synthetic contract fixture** (its own source header: "Synthetic M9 reference-workload contract binary … deterministic stand-in for the full reference-workload harness") | `usr/bin/refwork-harness` (835,272 B) — the **real reference-workload emulator** |
| boot.toml | 616 B, execs `/opt/m9-refwork-contract` | 1,242 B, execs `refwork-harness`, `game_source="pv-blk"`, regions wram/framebuffer/meta |

The resolution booted the M9 refwork *contract fixture* (which emits frame marks
by construction), not the real emulator in the deployed snapshot.

## A/B reproduction on THIS branch (891c5b6), only the initramfs swapped

`linux_m5_frame_budget_records_post_ready_frame_marks --ignored`, identical
`bzImage`/`base.img`/`game.img`, fresh empty `DH_M9_IMAGE_CACHE` each run — so
the initramfs is the only variable:

```
A) DH_M9_INITRAMFS = cache initramfs   (Jun-20, m9-refwork-contract fixture)
   => PASS: linux-m5 frames start=0
      first_frames=[(186992,1),(330795,2),(474598,3)]
      restored_frames=[(143803,4),(287606,5)]

B) DH_M9_INITRAMFS = real-emulator initramfs  (Jul-5, refwork-harness)
   => FAIL: "first Linux run stopped with reason 4, expected BudgetReached"
      (StopReason 4 = HARD_CAP, proto/hypervisor.proto:244; fails at
       m5_frame_scheduling.rs:87 — the assert on the FIRST fresh-boot
       Run{frame_budget}, before the test's TakeSnapshot/RestoreSnapshot arm)
```

Same worker code, same kernel/base/game; the real emulator does not reach a
`FRAME_COUNTER` under `dh-worker` no-tick on a **fresh boot**. Because it fails
at the first, pre-restore FrameBudget (and `m9_linux_ready_snapshot` reached
`Ready` first), boot-to-Ready succeeds and this is **not restore-specific**.

## The 2×2 — this refines H2, it does not reverse it

|  | fresh boot | restore |
|---|---|---|
| **guest-sdk VmHarness** | frames (fixture; real emulator w/ NOP ROM) | frames (guest-sdk resolution) |
| **dh-worker** | fixture: frames · **real emulator: HARD_CAP** | fixture: frames · real emulator: n/a (fails at boot) |

`00-overview.md`'s H2 ("fresh-boot HARD_CAP through dh-worker") was real and was
on the real emulator. But the new data point — **dh-worker frames the
`m9-refwork-contract` fixture fine** — narrows it: this is **not** a categorical
dh-worker no-tick frame/drain failure. It is **conditional on the real
emulator**. That conditionality raises the prior that the root cause is a
**guest/host-contract seam** (something the real emulator does that the fixture
does not), which could land in reference-workload / guest-sdk rather than
dh-worker. We are not asserting a dh-worker defect; we are asserting the
observable is still broken and the owner is not yet known.

## On the harness comparison — apparent, not yet isolated

The real-emulator initramfs also frames in guest-sdk `VmHarness` no-tick
(`refwork_ready_hold::no_timer_real_harness_reaches_and_holds_ready`, meta frame
counter advances `f1 > f0`). That looks like a dh-worker↔VmHarness divergence,
**but it is not a controlled comparison**: that VmHarness run attaches
`nop_rom()` (a 32 KiB NOP stub, `refwork_ready_hold.rs:38-44,100`), whereas
dh-worker case B feeds the real `game.img` via pv-blk. So harness **and** game
both differ between the "frames" and "HARD_CAP" legs — the emulator binary is
identical, but it runs different game code. Treat the divergence as *apparent
and not yet isolated*; the load-bearing conclusion rests on the clean A/B above
(initramfs the only variable), not on this cross-comparison.

## What we need next (reopen, don't close)

1. **Re-run the red repro against the real emulator, from a reproducible
   source** (not a private scratch copy). Build it and confirm red on this
   branch:
   - `reference-workload` @ **`7e94a82`** with `image/guest-sdk.lock` pinned to
     rev **`487ff564fe2b67dc2aef59aa90d31a74fc86c028`** → `cargo xtask image build`
     → `zstd -d dist/workload-image-0.1.0/initramfs.cpio.zst`. The build is
     byte-reproducible; it yields sha256 `b6f5a7cd…` (the deployed initramfs).
   - **Verify by payload, not just hash:** the initramfs must contain
     `usr/bin/refwork-harness` (the real emulator), *not*
     `opt/m9-refwork-contract`. Keep `bzImage`/`base.img`/`game.img` as in
     `08-resolution.md` (identical). The same cpio is embedded in deployed
     snapshot `1499c0a7…`.
2. **Localize the divergence (joint diagnostic, owner TBD).** Determine what the
   real emulator does between `Ready` and its first `FRAME_COUNTER` that the
   fixture does not, and why it advances in VmHarness but not dh-worker.
   `dh-worker` discards guest serial; `VmHarness` exposes it (`vm.serial_text()`),
   so a side-by-side serial/event capture at the point the emulator stops
   emitting is the fastest lead. A controlled VmHarness run with the **real**
   `game.img` (not `nop_rom()`) would also isolate whether the game or the
   harness is the variable.
3. **Follow the cause wherever it lands.** If it is a guest-side contract the
   real emulator violates under no-tick (e.g. a serial/timing dependency the
   deterministic worker legitimately does not service), the precise contract
   statement should go to reference-workload / guest-sdk — this is a co-equal
   outcome, not a fallback. If it is a dh-worker host-side gap, fix it there.
   Either way the observable to close is unchanged: the **real** emulator,
   freshly booted under `dh-worker` no-tick, must reach `BUDGET_REACHED` with
   `frames_elapsed == N`.

Tracking: rom-operator-bridge-9xo. Deployed snapshot ref
`1499c0a77e883bc0f74d97a254e59508ea86b4d17976eba8cbf78e0c7961a270`.
