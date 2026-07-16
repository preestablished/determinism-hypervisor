# Suggestions (non-blocking)

## S1. `step_one_entry` doc-comment vs. the generic on_exit contract

The doc says the call returns "at the next debug trap" and frames the boundary as
"one entry." That is true for the sole caller (`run_segment`, where `exits!()`
turns `Hlt` into an `Err` so the loop cannot re-enter). But a *future* caller
that returns `Ok(())` from `on_exit` on a `Hlt` (or any non-step exit that
re-enters) would silently run more than "one entry" — the loop keeps spinning
until a `Debug` exit. This is a latent foot-gun, not a current bug.

Suggestion: add one sentence to the doc making the precondition explicit, e.g.
"The caller's `on_exit` MUST treat any exit that would *re-enter* the guest
(notably `Hlt`) as terminal (return `Err`), otherwise more than one entry can
elapse before the next `Debug` trap." This documents the contract the engine
actually depends on rather than relying on the caller getting it right by luck.

## S2. `'arm'` mode is dead until bead 40q — guard against silent rot

`.arm_mode` (the MMIO pv-clock arming loop) cannot run under today's debug loops:
its first `mov [rbx + CLOCK_DEADLINE], rax` is an MMIO write that surfaces as a
loud foreign exit (`no_exits` would fail it). The asm comment and the 40q note
already flag this, which is acceptable. To keep it from rotting undetected, a
cheap option is a `#[test]` (or a comment-anchored assertion) that loads the
guest with `b"arm"` and asserts it reaches the first MMIO exit at the expected
RIP / clock address — i.e. a smoke test that the mode-dispatch and the loop
preamble are still wired, without needing the device-bus run loop. Optional;
fully fine to defer to 40q.

## S3. Cmdline first-byte dispatch is single-char and silent on unknown

The mode select reads only `cmdline[0]` and compares against `'m'`/`'a'`. A
cmdline like `"armadillo"` selects arm-mode; an unknown first byte falls through
to `.open_window` (STI + spin), which is a reasonable default. This is fine for a
test guest. If the guest ever takes richer cmdlines, consider matching the full
token. No action needed now — noting it so the convention is intentional rather
than accidental.
