# Suggestions (non-blocking)

All five are quality/robustness/doc — none affect correctness of the landed boundary.

## S1. The post-loop `set_singlestep(false)?` can replace an `Ok` boundary with an `Err` — document it

`boundary.rs:163-169`:

```rust
if stepping {
    set_singlestep(&mut guard, false)?;   // <-- ? discards a successful `result`
}
result
```

If the boundary was reached (`result = Ok(boundary)`) but the final `KVM_SET_GUEST_DEBUG control=0`
ioctl fails, the `?` returns `Err(BoundaryError::Kvm(...))` and the *correct, exact* boundary is
thrown away. This is the **conservative** choice — a vCPU stuck in single-step is far worse than a
recomputable boundary, and `set_guest_debug(control=0)` failing is essentially "KVM is broken" — so I
am NOT asking to change the behavior. But the intent is non-obvious; a one-line comment ("disable
failure shadows even a good landing — intentional: a vCPU left in single-step is unrecoverable")
would stop a future maintainer from "fixing" it into a silent leak of the debug state. Suggestion-only.

## S2. No defensive single-step reset at entry — judge as acceptable, document the precondition

`land_at` assumes the vCPU is *not* already in `KVM_GUESTDBG_SINGLESTEP` on entry. The engine's own
paths always disable it before returning, so within this module the invariant holds. But if some
*external* caller (a future bisection helper, manual debug, a different code path) leaves single-step
enabled and then calls `land_at` with a far target, the far approach's `guard.run()` would take one
KVM_EXIT_DEBUG immediately instead of running to the PMI — still correct (EINTR/Debug both loop and
re-read), just slower, and a surprise. I judge a defensive `set_singlestep(false)` at entry to be
**mild over-engineering** given the documented preconditions; the cheaper fix is to state the
precondition explicitly in the doc comment ("the vCPU is at an instruction boundary AND not in
single-step on entry"). Pick one; documenting is enough.

## S3. The `on_exit` callback contract for HLT during landing is under-specified

`boundary.rs:132, 155` route every non-debug, non-kick exit (including `VcpuExit::Hlt`) to the
caller's `on_exit`. The in-tree tests use `no_exits`, which treats ANY exit as fatal — appropriate
for the landing_loop guest, which never HLTs before the loop finishes. But the M3 scheduler will land
inside real guests that *can* HLT (idle), and the contract for what `on_exit` should do with an HLT
mid-landing (resume? treat as a stop? inject a wake?) is not written down. Add a sentence to the
`land_at` doc: "`on_exit` owns the policy for `Hlt` and device exits encountered *during* a landing;
returning `Ok(())` resumes the landing, `Err(..)` aborts it." This is a §3.3/§3.4 composition concern
and only needs a doc note now.

## S4. `Boundary` carries `rcx` for diagnostics but nothing asserts the REP invariant in-tree

The module docs and §3.1 make `rcx` a REP-progress snapshot, diagnostics-only, with the canonical
identity being `(icount, rip)`. The landing_loop guest contains no REP string ops, so no in-tree test
ever exercises the "RIP unchanged across a single step → keep stepping, don't declare a boundary"
path. This is fine for *this* bead (the REP rule is structurally enforced by trusting the counter),
but the REP behavior is currently **unverified by execution**. Suggest a follow-up bead: add a
nanokernel program with a `REP MOVSB`/`REP STOSB` and a test that lands mid-REP and asserts the
boundary is declared only on RIP advance. (Cross-references bead 8g1's torture scope.)

## S5. `BoundaryError::Kvm(String)` / `Exit(String)` allocate on the error path — minor

Stringly-typed error variants are fine for a fatal, rarely-hit path, but they allocate and lose
structure (e.g. the `Overshoot` variant is nicely structured; `Kvm` is not). If these errors ever
feed a machine-readable divergence report (§9 verification), consider carrying the `errno`/context as
fields rather than a pre-formatted `String`. Low priority; cosmetic until a consumer needs to match on
the kind.
