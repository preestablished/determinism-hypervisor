# Overview

Branch: `ralph/iteration-139-m7-accept-throughput-soak-n-slots-x-1-jo`  
Date: 2026-06-16  
Reviewer: Claude Opus  
Overall verdict: REQUEST_CHANGES

This branch adds an operator-run M7 throughput soak script and updates runner/test-partitioning docs to describe it. The direction is sound: the script builds outside the measured window, forces the M7 ignored acceptance test to run with skip disabled, derives the default target from the configured slot count, and records elapsed-time throughput rather than assuming the requested duration. However, the core acceptance condition depends on sustained housekeeping-core `stress-ng` load, and the script currently launches `stress-ng` in the background without verifying that it started, stayed pinned, or remained alive through the measured interval. That creates a false-green path where the soak can pass without the required load. The docs also need one correction because the new script is not part of `cargo test --workspace` and does not self-skip like the surrounding test-partitioning text says.

Stats: 3 files changed, 190 insertions, 4 deletions, 1 commit (`a5c373c ralph: iteration 139 checkpoint - m7 throughput soak`).
