Branch: ralph/iteration-128-m8-verifyreplay-divergence-bisection-dia
Date: 2026-06-15
Reviewer: Claude Opus

Reviewed `main...HEAD` at `5ceb98d ralph: iteration 128 checkpoint - verify replay bisection diagnostics`.

Overall verdict: REQUEST_CHANGES

The branch adds divergence field plumbing, a serializable `RegDiff` shape, and service tests, but it does not satisfy bead `determinism-hypervisor-3l2`: `bisect_on_divergence=true` reports a synthesized 1024-instruction window instead of performing true replay bisection, and diagnostic payloads remain coarse placeholders.

Stats: 8 files changed, 436 insertions, 36 deletions, 1 commit.
