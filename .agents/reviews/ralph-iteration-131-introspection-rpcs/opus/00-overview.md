# Ralph Iteration 131 Introspection RPCs Review

- Branch: `ralph/iteration-131-introspection-rpcs`
- Base: `main`
- Date: 2026-06-15
- Reviewer: Codex Reviewer 1
- Verdict: REQUEST_CHANGES

This branch wires the paused-slot introspection RPCs and detchannel event buffering in the right general area: raw/region memory reads are bounded, detchannel payload bytes are routed through the canonical guest-sdk encoder, and the run path now attempts to drain guest events at the pause boundary. I am requesting changes because the new pause-drain path mixes cumulative icounts with the segment-relative DHILOG writer, which can make post-restore segments unsealable or unreplayable and reports `StreamGuestEvents` positions in inconsistent domains; additionally, `GetFramebuffer` returns zero metadata and the whole framebuffer region as pixels instead of decoding the descriptor required by the API/architecture contract.

## Stats

- Commits reviewed: 1 (`8e1ddca ralph: iteration 131 checkpoint - introspection rpcs`)
- Files changed: 3
- Diff size: 609 insertions, 30 deletions
- Changed files read in full:
  - `crates/dh-devices/src/detchannel.rs`
  - `crates/dh-worker/src/runtime.rs`
  - `crates/dh-worker/src/service.rs`
- Required review inputs read: `git diff main...HEAD`, `git diff main...HEAD --name-only`, `git log main..HEAD --oneline`, and full contents of all changed files.
- Targeted tests run:
  - `cargo test -p dh-worker capture_size_limits_reject_oversized_lengths`
  - `cargo test -p dh-devices truncated_and_non_utf8_events_digest_identically`
  - `cargo test -p dh-worker introspection_rpcs_read_memory_framebuffer_and_stream_guest_events`
