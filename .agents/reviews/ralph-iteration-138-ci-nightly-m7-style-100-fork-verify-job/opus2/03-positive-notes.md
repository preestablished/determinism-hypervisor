# Positive Notes

- `.github/workflows/nightly-drift.yaml:118-149` adds the M7 canary with the right operational shape for this runner: it is gated by `determinism-class`, runs on `[self-hosted, kvm-intel]`, checks `/dev/kvm` and `nasm`, and runs the ignored M7 harness in release mode.

- `.github/workflows/nightly-drift.yaml:126-127` uses scheduled/manual-safe defaults for the new M7 inputs. This matches the existing `inputs.fuzz_runner || 'ubuntu-latest'` and `inputs.fuzz_seconds || '3600'` pattern in the same workflow, so scheduled runs still resolve to concrete values.

- `.github/workflows/nightly-drift.yaml:220-232` correctly includes `m7-fork-verify-100` in the alert job's `needs` list and updates the title/body text. A failed M7 canary is therefore part of the existing `failure()`-driven issue path.

- `crates/dh-worker/tests/m5_net_loopback.rs:150-159` updates the first manual quantum to use `run_segment_with_epoch_options` and suppress only the non-epoch final stop hash before applying canonical `NET_RX`. That matches the replay engine's canonical record path, where `NET_RX` records are reached with `hash_final_stop=false` before the record is applied.

- `crates/dh-worker/tests/m5_net_loopback.rs:289-320` keeps strong assertions around the recorded log shape: exactly one canonical `NET_RX`, exactly one AUX `NET_TX`, `NET_RX` landing one icount after `NET_TX`, and nonzero epoch hash coverage.

- `crates/dh-worker/tests/m5_net_loopback.rs:395-402` preserves the most important end-to-end check after the boundary-contract fix: replay must apply the `NET_RX`, verify every recorded epoch hash, match the end icount/hash, and reseal byte-identically.

- `docs/ops/github-runner.md:109-114` documents that the new nightly M7 coverage is live and ties `DH_M7_ACCEPT_SLOT_CORES=2-5` back to the actual isolated slot-core set on the runner.

- `docs/ops/test-partitioning.md:57` makes the scheduled 100-child M7 command visible in the hardware-gated test partition table, which gives operators a concrete reproduction command for the nightly lane.
