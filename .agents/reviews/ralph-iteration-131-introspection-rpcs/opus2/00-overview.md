# Overview

Branch: `ralph/iteration-131-introspection-rpcs`
Date: 2026-06-15
Reviewer: Codex Reviewer 2
Verdict: REQUEST_CHANGES

This branch wires the M6 introspection RPCs through the worker runtime: `ReadGuestMemory`, `GetFramebuffer`, and `StreamGuestEvents`, plus runtime-side storage for drained detchannel guest events and a detchannel helper that exposes canonical stream payload bytes. The implementation is close in its detchannel event encoding and unselected-event retention, but I found two important issues before this should pass the gate: `GetFramebuffer` returns non-empty pixels with zero width/height/stride despite the proto saying pixels are `stride*height`, and the paused-slot validation happens before the per-slot actor operation is queued, leaving a hidden concurrency assumption where racing write RPCs can change the boundary that introspection observes.

Stats: 1 commit reviewed (`8e1ddca ralph: iteration 131 checkpoint - introspection rpcs`); 3 files changed; diff stat is 609 insertions and 30 deletions across `crates/dh-devices/src/detchannel.rs`, `crates/dh-worker/src/runtime.rs`, and `crates/dh-worker/src/service.rs`. Review inputs read: `git diff main...HEAD`, `git diff main...HEAD --name-only`, `git log main..HEAD --oneline`, and the full changed-file contents. Tests were not run because this was a review-only pass.
