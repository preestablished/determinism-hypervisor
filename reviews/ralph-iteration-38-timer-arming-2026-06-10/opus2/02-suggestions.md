# Suggestions (non-blocking)

### S-1. `start_icount + 1` in the clamp can overflow in debug builds at the u64 edge

**Where:** `crates/dh-vmm/src/runctl.rs:123` — `.max(start_icount + 1)`.

The clamp uses a bare `+ 1`. For a segment whose `start_icount == u64::MAX` this panics in debug (overflow) / wraps to 0 in release. In practice unreachable (a segment starting at `u64::MAX` has no room to retire), and the agenda's own `checked_add` (agenda.rs:99-106) would reject the budget first — but the converter is a standalone `pub fn` callable independently, and the rest of the codebase is fastidious about `checked_*`/`div_ceil`/`saturating_*` at the u64 edge (see agenda.rs:163-170, clock.rs:84-86, vt.rs:43-55). For consistency with that house style, use `start_icount.saturating_add(1)` or `.checked_add(1).ok_or(RunError::ClockOverflow)?`. Saturating is the lighter choice and matches the "stop at the boundary" intent.

### S-2. The live test name promises more than it asserts; add the deferral counterpart

**Where:** `crates/dh-vmm/src/runctl.rs` `armed_timer_fires_and_reports_live`.

The test name says "fires and reports" but the budget == deadline construction deliberately *avoids* the deferral path (it merges injection and final stop, so the queued vector never enters the empty IDT). That's a deliberate, well-documented choice and the right one for a no-IDT guest. But it means the run-loop path that defers (window closed at the converted boundary) is **not** covered through `run_segment` — only `inject.rs`'s own `sti_tests` cover deferral, and those call `inject_at_boundary` directly, not the timer chain. Consider a follow-up test (gated behind bead 583's IDT-equipped guest) that arms a timer with budget **>** deadline and IF=0 across the deadline, so `delivered_icount > armed_deadline_vns` is observed *through* the timer chain. This is forward work, appropriately blocked on 583; a one-line `// TODO(583): deferral through the timer chain` near the test would record it.

### S-3. `timer_slot == Some(*idx)` comparison is correct but fragile to future multi-timer changes

**Where:** `crates/dh-vmm/src/runctl.rs:203-209, 290-296`.

The timer is appended last (`all_injections.push(...)`, slot = `len - 1`) and matched by index equality. This is correct for exactly one timer. If a future change ever supports more than one armed source per segment, the single `timer_slot: Option<usize>` and the single `seg.timer.expect(...)` would silently mis-attribute. Not a bug today (one-shot, single timer), but a `// INVARIANT: exactly one timer slot; revisit if multi-timer lands` comment at line 203 would flag the assumption for the next editor. Low priority.
