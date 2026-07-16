# Review Resolution

Two independent reviews ran against the plan at commit `f58ac28`:
`01-contract-correctness-review.md` (contract fidelity + code claims) and
`02-feasibility-tests-review.md` (fixture feasibility + test coverage). Both
verdicts: **implementable as written**, with fixes. Every finding below is
now folded into plan files 00–05; this file records the disposition.

## Disposition table

| # | Finding (reviewer) | Severity | Resolution |
|---|---|---|---|
| 1 | `capture_epoch_leg` `IcountBudget(100_000)` insufficient after fixture resize (fill loop grows 32,768 → ~114,688 instructions before manifest publish); breaks `m6_accept_capture_neutrality_and_layout_precondition` at KVM runtime with no compile error (R2-F1) | Critical | **Accepted.** Plan 04 now mandates raising the budget to ≥ ~500k, with the icount arithmetic and the epoch_len=64 note. Plan 01's runtime-test inventory lists the breaking test. |
| 2 | Deletion inventory missed `tests/nanokernel/tests/elf_shape.rs` (lines 61, 425–497, and the test-time-only string entry at 511) and `capture_manifest_interop.rs`; plan 01's "verified by grep across crates/" claim was false (R1-F1, R2-F2) | Important | **Accepted.** Plan 01 gained a "Nanokernel crate's own tests" section; plan 04's deletion checklist now enumerates all elf_shape.rs sites and flags the no-compile-error entry; interop test noted as auto-adapting. |
| 3 | m6 `capture_spec()` question left open when `framebuffer: false` (m6_full_api_uds.rs:135) resolves it — no fb-assertion changes needed in m6 (R2-F3, R1 verified same) | Important | **Accepted.** Resolved in plans 01 and 04; implementer told not to burn a gated run on it. |
| 4 | Golden/hash tests do not pin capture bytes — plan claim verified in its favor (R2-F4) | Important (confirming) | **Recorded.** Plan 03 Notes and plan 04 consumer sweep now cite the verification instead of hedging. |
| 5 | Plan 01 runtime-test inventory omitted four capture_fixture tests (7482, ~7664, ~7695, ~7792) (R2-F5) | Minor | **Accepted.** Added to plan 01 with the verified "latter three survive unchanged" note. |
| 6 | NASM `229_376` underscore literal would panic elf_shape's `%define` parser (R2-F6) | Minor | **Accepted.** Plan 04 says write `229376` or `0x38000`. |
| 7 | Stale "descriptor" wording in `docs/ops/m6-grpcurl-metrics-smoke.md:146` (R2-F7) | Nit | **Accepted.** Added to plan 04. |
| 8 | Plan 00 endorsed a request error: black frames do NOT "silently emit zero FbInfo" — `known_format` includes `PfUnspecified`, so all-zero frames fail loudly; the silent fallback needs unknown format AND implausible dimensions (R1-F2) | Minor | **Accepted.** Plan 00 softened; plan 01 gained a "Heuristic behavior correction" block; plan 05 warns not to copy the wrong narrative into the decision record. |
| 9 | "Errors propagate out of Run/TakeSnapshot is the request's explicit ask" overstated; also a failed-capture Run has already executed the guest (caller loses RunResponse) (R1-F3) | Minor | **Accepted.** Plan 03 step 4 reworded (plan's choice, consistent with request); behavior-change note added to plan 05's handback contents. |
| 10 | "Capture output feeds snapshot artifact bytes" wrong — `take_snapshot_with_lapic` (service.rs:4193) never consumes `CaptureOutput`; capture output is RPC-payload-only (R1-F4) | Minor | **Accepted.** Plan 03 Notes corrected; 3× workspace runs kept as mandatory process, with the fixture-resize icount change as the remaining determinism-adjacent surface. |
| 11 | `FramebufferCaller` prefix vs callerless builder signature inconsistency (R1-F5) | Nit | **Decided.** Thread `FramebufferCaller` into the new builder; plan 02 and plan 03 step 3 updated. |
| 12 | Zeroed-region coverage is unit-level only; no end-to-end black-frame window exists in the fixture (R1-F6) | Note | **Accepted.** Recorded in plan 04 and in plan 05's handback contents (don't overclaim coverage). |

## Not adopted

Nothing — no finding was rejected. Both reviews also independently confirmed
the load-bearing plan decisions: exact-length check semantics, fixture
resize memory-map safety (region ends 0x63_8000 inside 8 MiB guest RAM),
absence of hidden capture-output consumers, and the bd/docs/process steps.
