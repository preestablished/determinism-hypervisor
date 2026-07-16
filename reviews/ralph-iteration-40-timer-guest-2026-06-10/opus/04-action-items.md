# Action Items

### Critical

None.

### Important

None.

### Suggestions

1. **Tighten the `step_one_entry` doc-comment contract.**
   In `crates/dh-vmm/src/boundary.rs`, add a sentence to the `step_one_entry`
   doc making the caller precondition explicit: any exit that would *re-enter*
   the guest (notably `Hlt`) MUST be treated as terminal by `on_exit` (return
   `Err`), or more than one entry can elapse before the next `Debug` trap. The
   sole current caller (`run_segment` via `exits!()`) already does this; the doc
   should state the invariant rather than rely on it implicitly.

2. **Add a cheap smoke test (or anchor) for `'arm'` mode so it cannot rot.**
   `.arm_mode` in `tests/nanokernel/asm/timer_guest.asm` is dead until bead 40q
   (device-bus run loop). Optionally load the guest with `b"arm"` and assert it
   reaches the first MMIO exit at the expected RIP / clock-deadline address, so
   the mode-dispatch and loop preamble stay verified without the full run loop.
   Fine to defer to 40q.

3. **Note the single-char cmdline dispatch convention.**
   The mode select in `timer_guest.asm` matches only `cmdline[0]` against
   `'m'`/`'a'` and defaults unknown bytes to STI+spin. Acceptable for a test
   guest; if cmdlines ever grow richer, match the full token. No change needed
   now — recorded so the convention is intentional.
