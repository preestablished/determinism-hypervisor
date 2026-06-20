# Implementation Feasibility Review

Reviewer: `019ee59d-d8ce-7ed0-b26f-99c8825793cd` (`Socrates`)

Status: request changes

## Findings

- High: The plan mixed cumulative worker counters with DHILOG segment counters. Phase 5 said to store Linux `run.icount` and `run.vns` as child end values, and the contract checked `RunResponse.icount <= hard_cap`. `RunResponse.icount` and `vns` are cumulative, while sealed DHILOG `end_icount` and `end_vns` use the segment counter reset by `TakeSnapshot`. The plan should add `root_cumulative_icount` and `root_cumulative_vns`, compute `segment_end_icount = run.icount - root_cumulative_icount` and `segment_end_vns = run.vns - root_cumulative_vns`, and use only segment values for lineage, header, and VerifyReplay checks.

- Medium: The proposed flat `AcceptanceHarness` could create Rust ownership problems. `M9LinuxReady` already owns the Linux service, blocking snapstore client, tempdir, store runtime, and lease resources. The plan should specify an enum-backed harness with `Nanokernel { ... }` and `Linux { ready: common::M9LinuxReady }`, plus accessors.

- Medium: The Linux log parser work was underspecified. The M5 record/replay parser collects epoch hashes but ignores frame marks. The plan should define `ParsedChildLog` with header fields, epoch hashes, frame marks, canonical count, and log hash. It should validate frame marks exactly like the frame-scheduling helpers: expected frames are `ready_frame_counter + 1..=ready_frame_counter + M9_LINUX_CHILD_FRAMES`, with strictly increasing icounts.

- Low: The nightly workflow plan adds `m7-linux-fork-verify-100` but did not say to update `alert-on-failure.needs`. The alert issue title/body should mention the Linux M7 canary too.

- Low: The baseline command for the current Linux guard used `--list`, which does not execute or prove the panic path. The plan should run the ignored guard test and expect failure before implementation.

## Overall

The plan fits the codebase directionally: explicit-core M9 READY setup is needed, M7's nanokernel-only paths are the right refactor target, and workflow/docs updates are scoped correctly. The main blocker was the cumulative-vs-segment counter issue; the plan needed that fixed before handoff.
