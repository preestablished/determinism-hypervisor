# Action Items

### Critical
- None.

### Important
- None.

### Suggestions
- [ ] **Collapse the 4× repeated event/halt unwind arms.** In
  `crates/dh-vmm/src/runctl.rs`, the `Err(_) if halted => finish_at_counter(GuestHalted,…)`
  / `Err(_) if event_stop => finish_at_counter(event_reason,…)` / `Err(e) => …` block is
  duplicated verbatim at four guest-executing call sites (`land_at` ~371, `step_one_entry`
  ~404, `inject_at_boundary` ~440, pause roll-forward ~537). Extract a local macro/helper
  that performs the halt-then-event-stop check so each site reduces to one line. This is a
  maintainability win only — a future arg added to `finish_at_counter` currently needs four
  edits. (See 02-suggestions.md S1 for a concrete macro shape.)
- [ ] **De-duplicate the double `ok_or` for the SDK feed.** In
  `crates/dh-vmm/src/runctl.rs:277–284`, `seg.sdk_events.ok_or(MissingSdkEventFeed)?` is
  written twice in one expression. Bind the cell once (`let cell =
  seg.sdk_events.ok_or(…)?; Some((cell, cell.get()))`). Cosmetic.
- [ ] **(Optional) Clarify the `frames_elapsed` field doc** in
  `crates/dh-vmm/src/runctl.rs:82–86` to lead with the general "count of FRAME_COUNTER
  exits in EVERY mode" meaning before the FrameBudget-specific `== frames` case. Doc-only,
  lowest priority. (02-suggestions.md S4.)

---

**Overall:** No blocking work. The branch is approved to merge as-is; the three suggestions
above are quality-of-life cleanups that can be folded into this iteration or a follow-up at
the author's discretion.
