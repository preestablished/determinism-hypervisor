# Suggestions (optional, non-blocking)

All of these are nice-to-haves. None block merge.

## S1 — The `gettid` doc comment is misleading

In `tests/determinism/tests/regression.rs`, the comment above `gettid()` reads partly as
a stream-of-consciousness note ("std exposes no gettid; tests run single rig per thread,
and the counter must route to THIS thread."). The function is correct — it calls
`libc::syscall(SYS_gettid)` to get the real kernel TID for `route_overflow_to_thread` — but
the comment's middle sentences don't parse cleanly. Suggest tightening to one sentence:
"std exposes no gettid wrapper, and tests run on worker threads (tid != pid), so route
overflow to this thread's real kernel TID via the syscall." Pure readability.

## S2 — Consider a brief assertion message on the 5-tuple smoke test

`ten_million_twice_equal_final_hash` uses a bare `assert_eq!(a, b)`. The 1e9 gate has a
helpful "DETERMINISM REGRESSION (P0)" message; the smoke variant does not. If the smoke
test ever fires in fast local iteration it would be nice to get the same loud framing.
Trivial; optional.

## S3 — Document the budget/HLT margin assumption near the cmdline constant

`ITERS_CMDLINE = b"125000000"` is derived as 1e9 / 8 and relies on the guest having
*strictly more* than `budget` retirable instructions (prologue + 1e9 loop + epilogue) so
the run stops on `BudgetReached` rather than `GuestHalted`. This invariant is currently
documented only in `landing_loop.asm`. A one-line comment at `ITERS_CMDLINE` — e.g. "must
yield > budget retirable instrs so the run lands on BudgetReached, never GuestHalted" —
would make the coupling explicit at the test site and guard against someone lowering the
iteration count without realizing it would flip the stop reason. The current code already
fails loudly if violated, so this is purely defensive documentation.
