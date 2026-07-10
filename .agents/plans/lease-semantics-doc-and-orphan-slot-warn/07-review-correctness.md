# Correctness And Acceptance Review

Reviewer: `/root/review_correctness` (independent subagent)

Verdict: `REQUEST_CHANGES`

## Findings

1. **High - AC3 required delivery, not merely a local copy-ready note.** The
   draft weakened the source request by treating the local resolution as the
   orchestrator handback and allowed `umay` to close without evidence that
   `w1v` was annotated.
2. **High - AC4 required operator notice, not merely tracking.** The operator's
   response/sign-off is non-blocking for deferral, but the notice itself must be
   sent before the acceptance criterion is complete.
3. **Medium - release-path wording was overbroad.** `DestroyVm` is the only
   client-invoked normal release for retained VM leases, but internal rollback,
   `VerifyReplay` temporary cleanup, and host-integrity teardown also release
   slots.

## Positive Audit

The reviewer confirmed that the plan covers all production `NoFreeSlot`
surfaces, the classifier and per-slot base payload match the request, the fake
delta matches current behavior, and deferral is justified by the absent
sweep/renewal/session/auth/runtime-coordination infrastructure.
