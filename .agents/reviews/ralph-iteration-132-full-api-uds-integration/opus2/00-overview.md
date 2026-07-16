# Overview

Branch: `ralph/iteration-132-full-api-uds-integration`
Date: 2026-06-15
Reviewer: Claude Opus (2nd reviewer)
Verdict: REQUEST_CHANGES

This branch adds an ignored x86_64 M6 acceptance test that drives `HypervisorWorker` over a Unix domain socket, starts the real in-process snapstore, creates a baseline snapshot, restores 64 slots, injects input, runs, snapshots with a `CaptureSpec`, destroys, and compares each public digest against a single-slot baseline. The test has the right broad shape and compiles, but it can report success without exercising the acceptance path when hardware gating is unmet, and several error paths can leave restored slots undisposed after partial failure. Those two issues weaken it as an acceptance gate for bead `bik`.

Stats: 3 files changed, 584 lines added, 0 lines removed, 1 commit.

Commands reviewed: `git diff main...HEAD`, `git diff main...HEAD --name-only`, `git log main..HEAD --oneline`.
