# Suggestions

1. `crates/dh-vmm/src/boot.rs:332`

Consider materializing a small guest GDT and setting `sregs.gdt` for the Linux path. KVM segment caches may be enough for the current first instructions, but the Linux boot protocol explicitly describes a loaded GDT for `__BOOT_CS`/`__BOOT_DS`; making it concrete removes an avoidable protocol assumption.

2. `docs/decisions/m9-linux-ready-and-block-device.md:49`

If the upstream planning docs still say virtio-blk, add a matching `docs/upstream-divergences.md` entry. The decision doc is clear, but the existing divergence workflow is where future sync conflicts are tracked.

3. `tests/determinism/tests/common/mod.rs:30`, `crates/dh-worker/tests/common/mod.rs:26`

The M9 artifact env-var helper is duplicated across two test common modules. Fine for this branch, but a small shared test-support helper would reduce drift once more Linux acceptance tests land.
