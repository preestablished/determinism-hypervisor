# Risks

## Acceptance false positive

The main risk is closing `determinism-hypervisor-pee` while the acceptance still does not prove the stated service-level epoch-hash contract. The child snapshot hash check is meaningful, but the epoch-hash comparison can be vacuous.

## Helper path drift

`capture_epoch_leg` builds a direct KVM/runctl harness with a hand-registered bus at `crates/dh-worker/src/service.rs:3076` through `crates/dh-worker/src/service.rs:3210`. That is useful as a low-level check, but it is not the `WorkerService::run` path. Future service-level regressions in runtime construction, scheduled-input run wrappers, or DHILOG epoch logging can be hidden by this helper.

## Runtime cost

The new acceptance is not ignored. On this KVM-capable host, the single targeted test took 18.13s. It performs multiple guest runs plus a snapstore-backed restore/run/snapshot flow. That may be acceptable for hardware-gated acceptance, but it is a noticeable addition to default `dh-worker` library test runtime on hosts where `runtime_tests_available()` returns true.

## Input coverage

The capture/no-capture service legs use the same base snapshot but no scheduled inputs. That covers a deterministic empty-input case, not input queue consumption or canonical input logging under capture.

## Layout-version coverage

The `layout_version` checks are directionally sound: `Run` is checked at `crates/dh-worker/src/service.rs:4074` through `crates/dh-worker/src/service.rs:4084`, and `TakeSnapshot` is checked at `crates/dh-worker/src/service.rs:4106` through `crates/dh-worker/src/service.rs:4115`. Existing nearby tests already cover more detailed failure behavior, including run-boundary commit after a run capture mismatch and no snapshot publication after a snapshot capture mismatch.
