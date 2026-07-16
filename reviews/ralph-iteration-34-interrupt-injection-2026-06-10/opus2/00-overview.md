# Review: §3.4 deterministic interrupt injection (iteration 34)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-34-interrupt-injection` vs `main`
- **Bead:** mny
- **Scope:** `crates/dh-vmm/src/inject.rs` (new, 290 lines) + `lib.rs` module wiring
- **Environment:** live `/dev/kvm` (Intel i5-8400, kernel 6.8.0-124), no in-kernel irqchip; all experiments reverted.

## Verdict

**Request changes.** The injection primitive, the injectability predicate, and the two
live tests are correct and well-reasoned for the cases they cover. But the
`request_interrupt_window = 1` line composes incorrectly with `land_at`'s `on_exit`
contract: I reproduced live that the moment a real guest opens its interrupt window
mid-deferral (the *normal* case — a guest doing STI to accept a timer interrupt), the
next `land_at` single-step KVM_RUN returns `KVM_EXIT_IRQ_WINDOW_OPEN`, which
`inject_at_boundary` routes to `on_exit` and treats as fatal. The vector is never
delivered. Both shipped tests miss this because the landing-loop guest never executes
STI, so the window-open exit never fires.

The function currently has **no callers** (library code awaiting run-control wiring), so
this is latent rather than actively breaking a live path — but it will break the first
real timer-injection use case, and the module doc actively claims the opposite ("the
stepped path re-checks anyway"). That combination — wrong behavior plus a doc that hides
it — is why this is Critical rather than Important.

## Stats

| Severity | Count |
|----------|-------|
| Critical | 1 |
| Important | 2 |
| Suggestions | 4 |
| Positive notes | 6 |

## What I verified live (scratch, reverted)

1. **IRQ_WINDOW_OPEN vs Debug during single-step** — with `request_interrupt_window=1`
   and `KVM_GUESTDBG_SINGLESTEP`, once the guest's IF is set and the shadow clears, KVM
   exits `IrqWindowOpen` (not `Debug`) on the next entry, with RIP pinned — it never
   single-steps again. (C1.)
2. **End-to-end deferral loop** on a `NOP;NOP;NOP;STI;jmp $` guest: 4 Debug steps, then
   the post-STI boundary (`injectable()==false`, shadow set) triggers a `land_at` whose
   KVM_RUN returns `IrqWindowOpen` → `on_exit` → fatal. Vector never delivered. (C1.)
3. **Double `KVM_INTERRUPT` without a run between** returns `Ok` both times (no EEXIST on
   this no-irqchip kernel; the second silently overwrites the first). The prompt's
   "second → EEXIST → loud error" assumption is **false** here. (I1 / S2.)
4. **Exception-range vector** `KVM_INTERRUPT(14)` returns `Ok` — no kernel validation;
   semantically wrong (would deliver as external IRQ 14, not #PF). (I2.)
5. **Full `-p dh-vmm` suite 3× — 60/60, no flakes.** The triple-fault test
   (`open_window_injects_and_delivers_live`) is stable; Shutdown is deterministic because
   delivery through the empty IDT faults before any retirement.
