# Critical & Important findings

## Critical

None.

---

## Important

### I1 — `step_one_entry` has the same disarm bug; latent for the device-bus run loop

**File:** `crates/dh-vmm/src/boundary.rs:224-263` (the loop at 231-248).
**Caller:** `crates/dh-vmm/src/runctl.rs:280` (chained-injection `i>0` path).

`land_at` was just fixed to re-arm single-step on `Ok(VcpuExit::Debug(_))`.
`step_one_entry` — the sibling engine that walks exactly ONE guest entry under
single-step to chain same-boundary injections — was NOT touched and still has
the pre-fix structure:

```rust
match guard.run() {
    Ok(VcpuExit::Debug(_)) => break Ok(()),   // <-- no re-arm; it just returns
    Ok(exit) => {
        if let Err(e) = on_exit(exit) { break Err(e); }
        set_singlestep(&mut guard, true)?;     // re-arm only after NON-Debug
    }
    ...
}
```

It re-arms TF only after servicing a non-Debug exit (line 243) — the iteration-50
"MMIO-write eats the trap" fix — but it never re-arms after a Debug exit.

The 4a3 root cause is precisely an emulator-delivered **Debug** exit (not a
non-Debug MMIO exit) that consumes the arming. `step_one_entry` returns on the
FIRST Debug, so within a single call it is safe (one entry, one Debug, return).
But its CALLER (`run_segment`, runctl.rs:273-310) invokes it repeatedly, once
per chained injection. The arming `step_one_entry` sets at line 230 only lasts
until the first Debug; on the NEXT `step_one_entry` call a fresh arming is set,
so the inter-call boundary is fine. The exposure is INSIDE one call: if the
single step that should fire the returning Debug instead lands the entry on an
emulated-MMIO instruction, the emulator's completion delivers the Debug exit —
which `step_one_entry` treats as "done" and returns from. Re-read its contract
(lines 208-217): the returned boundary "can be many retirements ahead" because
event delivery suppresses the step. An emulator-Debug-from-MMIO is a DIFFERENT
suppression: the entry didn't deliver an interrupt, it just stepped onto an
MMIO instruction, and the Debug that fired is the emulator's completion hook —
`step_one_entry` would declare that the entry "completed" and read a boundary
that may be only a partial entry, OR (if KVM delivers the MMIO exit first and
the Debug after re-entry) the on_exit branch runs, re-arms, and the NEXT
in-call step free-runs because... actually no — the on_exit branch DOES re-arm.
The precise failure is the narrower one: a step that lands exactly on the
emulated-MMIO instruction such that KVM reports `VcpuExit::Debug` for the
completion (the same path 4a3 measured in `land_at`) makes `step_one_entry`
RETURN early — under-stepping the entry — without ever servicing the MMIO. The
symptom would be a wrong `delivered_icount`/`delivered_rip`, not a loud
overshoot.

**Reachability TODAY: none.** The `i>0` chained path requires interrupt
delivery, which requires a guest that (a) builds an IDT, (b) STIs, and (c) the
host queues ≥2 vectors at one boundary. The only injecting guests are:
- `timer_guest` (asm:130-144) — ISRs `RECORD` into `TABLE_GPA = 0x200000`,
  which is **plain guest RAM**, not the MMIO hole. Its `arm` mode does MMIO
  (CLOCK_DEADLINE writes) but the asm header (asm:12-15) states arm mode
  "REQUIRES the device-bus run loop (bead 40q); under today's debug loops an
  MMIO access is a loud foreign exit" — i.e. not run under the landing engine.
- `sti_window` — spins post-STI, no MMIO in the delivery window.
- `pad_echo` (M5) — its header (asm:9-11) is explicit: "Polling only: no IDT,
  no STI ... the pad IRQ_VECTOR stays 0" — it never delivers interrupts, so
  the chained path is unreachable for it despite its dense MMIO.

So no committed guest can place a chained-injection entry adjacent to an
MMIO instruction. **This is a now-or-later judgment call, and the answer is
LATER — but file it.** The moment a device-driven guest delivers interrupts
AND touches MMIO in the same window (the M5/M6 device-bus run loop the
timer_guest `arm` mode is waiting for), this becomes live and will manifest as
a silent wrong-boundary, not a loud overshoot — the worst failure class for a
determinism platform.

**Recommendation:**
1. File a P1 bead now (suggested below in 04-action-items) capturing the
   structural parallel and the reachability gate.
2. Strongly consider applying the one-line fix in THIS iteration while the
   reasoning is loaded: in `step_one_entry`, the early `break Ok(())` on
   `VcpuExit::Debug` is what makes the re-arm "unnecessary" — but the SAFE
   change is to distinguish "this Debug is real forward progress" from "this
   Debug is an emulator MMIO completion." That distinction is the same one
   `land_at` punts on (it re-arms unconditionally). For `step_one_entry`,
   re-arming unconditionally is wrong (it would loop forever — the Debug is
   the loop's exit condition). The correct backstop is the one the bead 4a3
   notes already propose: after servicing an MmioWrite/MmioRead under this
   loop, the entry's "one step" semantics need a counter-based progress check,
   not a TF-trap-based one. At minimum, add a comment at line 233 documenting
   that an emulator-MMIO-completion Debug is INDISTINGUISHABLE from a genuine
   step-Debug here and that this is safe ONLY because no committed guest
   delivers interrupts adjacent to MMIO — with a pointer to the new bead.

This is the highest-value finding in the change and the one the first
reviewer is most likely to under-weight (the fix LOOKS complete because
`land_at` is the obvious landing path; the injection-chain sibling is easy to
miss).
