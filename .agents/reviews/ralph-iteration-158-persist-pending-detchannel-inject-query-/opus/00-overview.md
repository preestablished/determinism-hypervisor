# Review Overview

**Branch:** `ralph/iteration-158-persist-pending-detchannel-inject-query-`  
**Date:** 2026-06-21  
**Reviewer:** Claude Opus  
**Verdict:** APPROVE

This branch adds EVTC v2 detchannel serialization for drained-but-unanswered `InjectQuery` state while keeping legacy EVTC v1 restore compatibility. The implementation uses a deterministic `BTreeMap` mirror of pending injects, validates v1/v2 section shapes separately, restores pending entries into a direct-answer cache for the OUT/restore/IN gap, and keeps replay cursor consumption aligned through the existing `FaultPlan` path. I did not find Critical or Important correctness issues in the changed code; the only follow-up I recommend is adding explicit malformed-v2 pending-table tests so the new validation rules stay pinned.

**Stats:** 5 files changed, 261 insertions(+), 27 deletions(-), 1 commit.

**Commit Reviewed:** `6c50231 ralph: iteration 158 checkpoint - persist inject EVTC state`

**Files Reviewed:**

- `crates/dh-devices/src/detchannel.rs`
- `crates/dh-snapshot/tests/dhsnap_codec.rs`
- `crates/dh-worker/tests/linux_worker_api.rs`
- `docs/phase-2-exit-gate.md`
- `docs/upstream-divergences.md`
