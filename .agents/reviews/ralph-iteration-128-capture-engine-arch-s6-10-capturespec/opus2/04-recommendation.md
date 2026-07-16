# Recommendation

Verdict: `REQUEST_CHANGES`.

Required before merge:

1. Add DetChannel-aware replay handling and a regression that verifies a DetChannel/capture fixture log reaches `VerifyReplay::Done`.
2. Add explicit capture output size limits before allocating `feature_bytes` or framebuffer buffers.
3. Decide and test `Run + CaptureSpec` failure semantics so a committed run is not hidden behind a bare gRPC error.

Nice to have:

- Validate DetChannel PIO access width explicitly.
- Add a capture-neutrality test comparing capture and no-capture outcomes from the same base state.
