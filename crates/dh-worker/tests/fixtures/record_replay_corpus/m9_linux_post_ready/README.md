# M9 Linux post-READY record/replay corpus

This fixture intentionally checks in only `expected.txt`.

The corpus test boots the staged M9 Linux reference workload to READY on the
Linux/KVM reference host, records a deterministic post-READY frame-budget
segment, seals the stored DHILOG through `TakeSnapshot`, parses the stored log,
and verifies replay from `(READY snapshot, input_log_id)`.

The full Linux READY snapshot and recorded log are generated live because the
M9 artifacts are external staged inputs and the Linux snapshot is too large for
a normal source fixture. The manifest pins the staged artifact hashes,
determinism-class lock hash, machine config hash, READY and END snapshot refs,
DHILOG hash, END state hash, frame counter, pv-blk proof checksum, and every
recorded `EPOCH_HASH`.

Refresh the manifest only on the reference host with:

```bash
DH_WORKER_REGEN_M9_LINUX_RR_CORPUS=1 \
DH_M9_ALLOW_SKIP=0 DH_M9_GUEST=linux \
DH_M9_BZIMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/bzImage \
DH_M9_INITRAMFS=/home/infra-admin/.cache/dh-m9/reference-workload/initramfs.cpio \
DH_M9_BASE_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/base.img \
DH_M9_GAME_IMAGE=/home/infra-admin/.cache/dh-m9/reference-workload/game.img \
DH_M9_IMAGE_CACHE=/home/infra-admin/.cache/dh-m9/image-cache \
cargo test -p dh-worker --test m5_record_replay --release \
  regenerate_m9_rr_corpus_manifest_for_reference_host -- --ignored --nocapture
```
