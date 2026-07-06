# Current evidence

## Reproduced red worker case

Using `reference-workload` at `7e94a828b2b9d252cff511cef5fc8baa4836caca`,
with the real-emulator dist initramfs decompressed to
`/tmp/dh-real-m9.DlWKwn/initramfs.cpio`:

```bash
DH_M9_ALLOW_SKIP=0 \
DH_M9_GUEST=linux \
DH_M9_BZIMAGE=/home/infra-admin/git/preestablished/reference-workload/dist/workload-image-0.1.0/bzImage \
DH_M9_INITRAMFS=/tmp/dh-real-m9.DlWKwn/initramfs.cpio \
DH_M9_BASE_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/base.img \
DH_M9_GAME_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/game.img \
DH_M9_IMAGE_CACHE=/tmp/dh-real-m9.DlWKwn/image-cache \
cargo test -p dh-worker --test m5_frame_scheduling \
  linux_m5_frame_budget_records_post_ready_frame_marks -- --ignored --nocapture
```

Result:

```text
Error: "first Linux run stopped with reason 4, expected BudgetReached"
```

Reason 4 is `HARD_CAP`; the failure is the first fresh-boot frame-budget run.

## Artifact provenance

The real dist initramfs contains:

```text
etc/detguest/boot.toml
init
usr/bin/refwork-harness
```

It does not contain `/opt/m9-refwork-contract`. Its `boot.toml` execs
`/usr/bin/refwork-harness` and uses `game_source = "pv-blk"`.

The stale local cache initramfs from the first attempt was 1,156,096 bytes and
execed `/opt/m9-refwork-contract`; it is not valid evidence for the deployed
real-emulator snapshot.

## Controlled guest-sdk probe

The guest-sdk `boot_probe` was run with the same real emulator initramfs, the
same real `game.img`, timer delivery suppressed, and timerless cmdline flags:

```text
BOOT_PROBE_GAME=/home/infra-admin/.cache/dh-m9/reference-workload/game.img
BOOT_PROBE_NO_TIMER=1
BOOT_PROBE_SECS=30
BOOT_PROBE_CMDLINE="console=ttyS0,115200 panic=-1 reboot=t 8250.nr_uarts=1 hugepages=4 notsc tsc=unstable clocksource=jiffies noapictimer lpj=4096"
```

Result: the VM reached `Ready` and drained 28 boot/region events, but no
`FrameMark` appeared within 30 seconds.

This invalidates the earlier uncontrolled comparison that used guest-sdk with a
NOP ROM and dh-worker with the real game. The real game/content path is now a
required variable to isolate.

## SDK frame-mark semantics

Current guest-sdk `frame_mark()`:

1. emits a ring-W `FrameMark` with `EventClass::Critical`;
2. writes pv-pad `FRAME_COUNTER`.

`EventClass::Critical` only rings `DOORBELL_RING_W` when the ring is full. On
the normal non-full path, there is no explicit doorbell. The guest-sdk harness
therefore drains at the `FRAME_COUNTER` MMIO exit. dh-worker currently drains
on detcall exits and pause boundaries, but not at the frame-counter exit.

The first plan's synthetic fixture rang the W doorbell before every
`FRAME_COUNTER` write, so it did not cover the SDK's normal path.
