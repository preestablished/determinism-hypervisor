# Critical & Important Findings

## Critical

None. The mechanism is sound and the live tests prove the property they claim.

---

## Important

### I-1. `step_one_entry`'s "one entry" contract is silently violated by an Ok-handled Hlt — doc the precondition

`step_one_entry` (`crates/dh-vmm/src/boundary.rs:19-49`) loops:

```rust
Ok(VcpuExit::Debug(_)) => break Ok(()),
Ok(exit) => { if let Err(e) = on_exit(exit) { break Err(e); } }
```

If `on_exit` returns `Ok(())` for a `Hlt`, the loop *re-enters the vCPU* and keeps running
until the next `Debug` trap — so a single "step_one_entry" call can span an unbounded number
of logical entries (HLT → re-enter → wake on the queued vector → ISR → trap). That is exactly
what the prompt flagged.

**At the only production call site it is safe**: `run_segment`'s `exits!()` wrapper
(`runctl.rs:234-244`) turns `Hlt` into `Err(BoundaryError::Exit("guest halted"))`, which
breaks the loop on the first HLT. I confirmed this. So *as wired today there is no bug*.

But the function is `pub` and its doc-comment ("enter once, return at the next debug trap")
does not state the precondition that makes "one entry" true. A future caller passing a
benign `on_exit` (e.g. one that services HLT as idle, like a real device-run-loop in 40q)
would get many entries under one call and silently overshoot.

**Action:** add a precondition line to the doc: *"`on_exit` MUST return Err for any exit
that would resume execution past an instruction boundary (notably Hlt); otherwise this runs
more than one logical entry."* Cheap, prevents a nasty future footgun. (Reviewer-1 may have
flagged the determinism angle; my angle is the contract-doc gap specifically.)

### I-2. `step_one_entry` reads the counter / regs only on the success path — but a deferral-window overshoot inside it is invisible to the Overshoot guard

Unlike `land_at`, `step_one_entry` has no `c > target` overshoot check — by design, since it
has no target. It trusts that exactly one debug trap ends the entry. That is correct for the
single-step + interrupt-delivery semantics. **However**, the boundary it returns
(`icount`, `rip`) is then fed straight into `inject_at_boundary` as the `at` for the *next*
vector. If the delivered entry ever ran into a region the M6 scheduler should have avoided
(the doc's own §3.2 NEAR-approach caveat at `boundary.rs:16-18`), there is no loud guard here
— the overshoot would only surface later as a `WindowNeverOpened` or a wrong hash.

This is consistent with the documented division of responsibility ("the M6 scheduler owns
avoiding such targets"), so it is **not a bug today** — M6 is not wired and the only callers
schedule both vectors at the *same* boundary. But it is the single place where the engine's
"never declare a boundary you didn't loudly verify" invariant is relaxed. I'd want a
debug-assert that the returned icount advanced by at least 1 (a zero-advance trap would mean
single-step fired without retirement — a REP-like or fault situation the chaining path does
not expect), so the relaxation stays honest:

```rust
debug_assert!(icount > /* entry icount */, "step_one_entry made no progress");
```

Requires threading the pre-entry icount in; low cost, makes the loosened invariant auditable.
Downgrade to Suggestion if the team prefers to keep the function target-free.
