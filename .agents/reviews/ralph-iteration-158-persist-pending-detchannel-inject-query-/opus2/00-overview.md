# Review Overview

**Branch:** `ralph/iteration-158-persist-pending-detchannel-inject-query-`
**Date:** 2026-06-21
**Reviewer:** Claude Opus (2nd reviewer)
**Verdict:** REQUEST_CHANGES

This branch correctly identifies the previously lossy OUT/restore/IN detchannel window and adds EVTC v2 state for drained-but-unanswered `InjectQuery` records while preserving v1 restore acceptance. The serialization and single-use restored answer path are generally well structured, but the v2 payload persists only `iseq` and `name_id`; after restore, the fresh `Channel` has an empty intern table, so name-specific `FaultPlan` decisions can diverge from an uninterrupted execution. That is a snapshot determinism issue and should be fixed before landing.

## Stats

- **Files changed:** 5
- **Lines added/removed:** +261 / -27
- **Commits:** 1 (`6c50231 ralph: iteration 158 checkpoint - persist inject EVTC state`)
