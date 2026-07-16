# 03-positive-notes.md

The test waits on `Until::NextSdkEvent` using only Ready stream matches, so serial output or non-Ready SDK events cannot satisfy the gate.

The identity comparison is direct and covers all requested fields: `ready_icount`, Ready payload `unit`, `region_count`, `manifest_generation`, `machine_config_hash`, and `state_hash`.

The new determinism test crate dependency on `detguest-wire` preserves the `dh-worker` architecture boundary; the helper duplicates only the minimum M9 wiring needed to avoid depending on worker internals.
