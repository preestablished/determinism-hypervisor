# Suggestions

### S-1: Cleanup failure on the Ok-landing path discards a successful boundary (judgment call — document the choice)

`boundary.rs:163-169`:

```rust
if stepping {
    set_singlestep(&mut guard, false)?;   // <-- the `?` here
}
result
```

If the loop produced `Ok(boundary)` but the final `set_singlestep(false)` ioctl
fails, the `?` returns `Err(Kvm("KVM_SET_GUEST_DEBUG: ..."))` and the caller
never sees the (real, correct) landed boundary. Conversely, if the loop already
produced an `Err`, a cleanup failure *replaces* that error too.

**This is arguably the correct conservative choice:** a vCPU left in
single-step is an R10 violation (TF leaking into guest-visible state on a later
resume), so refusing to hand back a "good" boundary while the vCPU is in a
dirty debug state is defensible — better a loud Err than a boundary the caller
trusts while the vCPU silently keeps trapping. The risk is the opposite of data
loss: it's losing a *valid* result to a cleanup hiccup.

**Recommendation:** Keep the behavior, but document it explicitly on `land_at`:
"a failure to drop single-step supersedes the loop result (incl. a successful
landing): we never return a boundary the caller would resume from a vCPU still
in SINGLESTEP." A future maintainer staring at "I landed but got an error"
needs that sentence. Optionally, distinguish the cleanup-failure error variant
(e.g. `Kvm` carrying a `landed: Some(Boundary)` breadcrumb) so run-control can
log the boundary it *would* have gotten — but that is gold-plating; the doc note
is the real ask.

### S-2: `arm_period` immediate-period semantics rest on the live tests — add one explicit assertion comment

`PERF_EVENT_IOC_PERIOD` on a running (enabled, guest-stopped) sampling event:
the kernel's `perf_event_period()` applies the new period to the *current*
accounting immediately (it does not wait for the next overflow to take effect).
The engine relies on this: arming `(d - skid)` after `c` instructions must fire
the PMI at roughly `c + (d - skid) = target - skid`, NOT at `c + (already
elapsed) + (d - skid)`. **The live tests are the empirical proof** — landing at
*exactly* 1,000,000 (test `lands_exactly_via_pmi_then_step_live`) is only
possible if the arm took effect from the current count within the 8192 skid;
if the period were measured from the prior overflow, the far approach would
overshoot or undershoot wildly and stepping would either Overshoot or take
millions of steps. It passed first-run. Suggest a one-line comment at
`counter.rs:140` or `boundary.rs:128` stating "IOC_PERIOD takes effect from the
current count (kernel perf_event_period); the exact-N landing tests are the
proof," so the assumption is documented, not folkloric.

### S-3: Stepping over a guest `HLT` near the target exits `VcpuExit::Hlt` → `on_exit` (document the boundary-vs-Hlt decision)

If the landing target sits at/just past a guest `HLT`, single-stepping that HLT
produces `VcpuExit::Hlt`, which `boundary.rs:155` routes to `on_exit`. The
landing tests use a tight loop with no HLT, so this never triggers there
(correctly). But run-control composition (§3.3) WILL step guests that HLT. The
current design — leave HLT to `on_exit` — is reasonable (HLT is a
guest/scheduler event, not a landing concern), but it is an implicit contract.
**Recommendation:** Add a doc line on `land_at` stating that HLT (and other
non-debug exits encountered mid-step) are delivered to `on_exit` and that the
engine does NOT special-case HLT, so the run-control author knows landing across
a HLT is their callback's responsibility. (Note: HLT retires once on the resume
that completes it — ARCH §3.1 — so counting stays correct; this is purely about
who services the exit.)

### S-4: Missing live coverage — landing while mid-flight exits occur (`on_exit` servicing)

All four tests use `no_exits` (any non-debug exit is a hard error). So the
`on_exit(exit)?` paths at `boundary.rs:132` and `:155` are exercised only for
their *error* branch, never for a "serviced, count unchanged, keep going" case.
The mid-emulation rule (an MMIO-exited instruction has not retired) is asserted
by the doc's `counting_semantics` test but not by `land_at` itself. Suggest a
future test (after the device run loop lands — see d-something device bead) that
lands `land_at` across a guest that does a handful of MMIO/PIO accesses, with an
`on_exit` that services them, asserting exact landing despite the interleaved
exits. **Not a blocker** — the device run loop isn't here yet — but it is the
one untested promise of the engine's contract. Track as a bead.

### S-5: REP-instruction landing has no live test

The module's headline correctness claim is the REP rule (boundary.rs:9-12):
debug traps per REP iteration don't retire; only counter+RIP-advance is
retirement. None of the four tests lands inside a REP string instruction (the
landing-loop guest is a plain counter loop). The bead references a guest (d34)
exercising REP. Suggest a live test that targets an icount that falls in the
middle of a `REP MOVSB`/`REP STOSB` run and asserts (a) it lands at exactly N,
(b) `b.rip` is the REP instruction's RIP and `b.rcx` reflects mid-progress, and
(c) no boundary is declared with RIP unchanged across a step. Track as a bead.

### S-6: `Boundary.rcx` is captured via `guard.get_regs()` at the boundary only — fine, note the cost contract

`boundary.rs:116-123` does exactly one `KVM_GET_REGS` at the boundary (not per
step) — correct and cheap. No change needed. Worth a one-line note in the bead
that diagnostics cost is O(1) per landing, so nobody later moves `get_regs`
into the step loop "to watch RCX" and turns landing into O(steps) ioctls.
