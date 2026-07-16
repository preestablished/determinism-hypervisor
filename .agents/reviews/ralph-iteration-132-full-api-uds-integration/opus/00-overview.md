# Overview

Branch: `ralph/iteration-132-full-api-uds-integration`

Date: 2026-06-15

Reviewer: Claude Opus

Verdict: REQUEST_CHANGES

This branch adds a new ignored x86_64 M6 acceptance test that drives the public worker gRPC API over a Unix domain socket, builds a base snapshot, compares 64 restored/injected/run/snapshotted/destroyed slots against a single-slot baseline, and adds the UDS client test dependencies to `dh-worker`. The core happy-path shape is aligned with bead `bik`, and the test does restore all 64 slots before proceeding, but the acceptance gate can silently return success when the required hardware is unavailable and several failure paths do not explicitly destroy already-leased slots, so this should not be accepted as a reliable acceptance gate yet.

Stats: 3 files changed, 584 lines added, 0 lines removed, 1 commit (`d8a76f7 ralph: iteration 132 checkpoint - full api uds acceptance`).
