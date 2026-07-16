# Suggestions (non-blocking)

These are maintainability / clarity improvements. None affect correctness.

## S1. The triple-arm unwind is repeated verbatim 4× — extract a helper closure

**File:** `crates/dh-vmm/src/runctl.rs:371–387, 404–427, 440–463, 537–559`

The same `Err(_) if halted => finish_at_counter(GuestHalted, …)` /
`Err(_) if event_stop => finish_at_counter(event_reason, …)` / `Err(e) => …` shape appears
at all four guest-executing call sites (`land_at`, `step_one_entry`, `inject_at_boundary`,
pause roll-forward). Each repeats the same six `finish_at_counter` arguments. This is the
single biggest maintainability cost of the change: a future field added to
`finish_at_counter` (or a new stop flag) must be edited in four places, and the arms are
easy to get subtly out of sync.

A local closure that maps the boundary-engine error to the right terminal outcome would
collapse all four to one line each. Because `finish_at_counter` borrows `seg` mutably and
the surrounding code also borrows `seg`, a closure capturing `seg` will fight the borrow
checker; the cleaner shape is a small helper that takes the relevant values and the
flags, e.g.:

```rust
// returns Some(result) when a sentinel flag fired, None to fall through to `Err(e) => `
macro_rules! event_or_halt_unwind {
    ($err:expr) => {{
        if halted {
            return finish_at_counter(seg, clock, StopReason::GuestHalted,
                                     delivered, timer_fired, frames_seen);
        }
        if event_stop {
            return finish_at_counter(seg, clock, event_reason,
                                     delivered, timer_fired, frames_seen);
        }
    }};
}
```

then each site becomes:

```rust
Err(_) if halted || event_stop => { event_or_halt_unwind!(()); unreachable!() }
Err(e) => return Err(RunError::Boundary(e)),
```

This is offered concretely as requested; it is purely a readability win and can be
deferred. The current code is correct.

## S2. `Segment::sdk_events.ok_or(MissingSdkEventFeed)` is evaluated twice

**File:** `crates/dh-vmm/src/runctl.rs:277–284`

```rust
let sdk_feed = match until {
    Until::NextSdkEvent { .. } => Some((
        seg.sdk_events.ok_or(RunError::MissingSdkEventFeed)?,
        seg.sdk_events.ok_or(RunError::MissingSdkEventFeed)?.get(),
    )),
    _ => None,
};
```

`seg.sdk_events` is `ok_or`'d twice in the same expression. It is harmless (the field is
`Option<&Cell<u64>>`, `Copy`, and the second `?` is unreachable once the first succeeds),
but a single bind reads cleaner and states the "fetch the cell, then read its baseline"
intent once:

```rust
Until::NextSdkEvent { .. } => {
    let cell = seg.sdk_events.ok_or(RunError::MissingSdkEventFeed)?;
    Some((cell, cell.get()))
}
```

## S3. `frame_mark_gpa` / `frame_target` / `sdk_feed` / `event_reason` are computed even when unused

**File:** `crates/dh-vmm/src/runctl.rs:272–289`

For `IcountBudget` / `VnsBudget` / `Goal`, all four of these are set to the "off" value
(`None` / unused `StopReason`) but the GPA arithmetic and the matches still run. This is
negligible cost and arguably clearer than nesting it under the event-mode branches, so
this is a very low-priority note — flagging only for completeness. No change needed unless
the run-segment prologue grows further.

## S4. Doc nit: `frames_elapsed` doc says "== `frames`" but the runtime guarantee is broader

**File:** `crates/dh-vmm/src/runctl.rs:82–86` (and API.md mirrors this)

The field doc says it equals `frames` "when a FrameBudget run stops BudgetReached". That
is true, but the more useful invariant the code actually provides — and that the tests
assert — is that `frames_elapsed` is the count of FRAME_COUNTER exits observed in EVERY
mode (e.g. an `IcountBudget` run that happens to cross two frames reports
`frames_elapsed == 2`). The doc does say "in EVERY until-mode" earlier, so this is only a
slight emphasis nit; consider leading with the general meaning. Non-blocking.
