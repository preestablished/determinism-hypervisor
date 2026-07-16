# Critical and Important findings

**None block the merge.** This section records the high-value correctness analysis
(including the explicit clearance of the highest-value question — whether the sibling
`step_one_entry` helper shares the bug) so it is not re-litigated next iteration.

## Correctness of the fix — CLEARED

The change adds, inside `land_at`'s stepping branch (`crates/dh-vmm/src/boundary.rs`
~line 177):

```rust
Ok(VcpuExit::Debug(_)) => {
    set_singlestep(&mut guard, true)?;
}
```

**Is the `Debug` arm guaranteed to run only while stepping?** Yes. The `match
guard.run()` containing this arm lives in the `else` block entered only when
`d <= skid_margin + resync_slack`, and the very first thing that block does is set
`stepping = true` and arm single-step (lines 152-161). The far-approach branch
(`!stepping && d > ...`, line 137) has its **own** `match guard.run()` with no `Debug`
arm at all — it never single-steps, so it can never receive a `Debug` exit and the
re-arm cannot fire there. A `Debug` exit while NOT stepping is therefore unreachable in
`land_at`. **Safe.**

**Idempotency / hardware-#DB case:** re-asserting `KVM_SET_GUEST_DEBUG` with
ENABLE|SINGLESTEP when the trap survived is a no-op for guest-visible state. Confirmed
harmless.

**Verified load-bearing (this review):** reverting the `set_singlestep` call in the
`Debug` arm makes both new regressions fail with loud `Overshoot` (4096→4110, 101→114);
restoring it makes them pass. The fix demonstrably closes the hole and the tests
demonstrably guard it.

## Highest-value question: does `step_one_entry` share this bug? — ANALYZED, NOT BROKEN

`step_one_entry` (`boundary.rs` ~line 224) is the *other* single-step loop. It is used
by `runctl.rs:280` to advance exactly one guest ENTRY between consecutive same-boundary
injections (so each queued vector delivers before the next is queued). It shares the
exact structural ingredient the fix is about: it single-steps and can cross an emulated
MMIO instruction, after which the emulator delivers a `Debug` exit.

The reason it is **not** broken the same way:

- `land_at`'s bug was *free-run past target*: it must keep stepping across MANY
  instructions, so an arming consumed mid-walk is fatal (it free-runs to the next
  natural exit). The fix re-arms so stepping continues.
- `step_one_entry` wants exactly ONE entry and `break Ok(())` on the **first** `Debug`
  exit. An emulator-delivered `Debug` after an MMIO completion is simply *the trap that
  ends the entry*. It stops the loop — the opposite failure mode from a free-run. It
  does not need to survive across instructions, so a consumed arming is irrelevant.

The one subtlety worth recording (already half-documented by the iteration-50 note at
lines 238-243): when the single entry's instruction is an MMIO write, the sequence is
enter → MMIO-write exit (handled, re-arm) → re-enter → trap after the *next*
instruction, so "one entry can span the write plus its successor." That is the existing,
accepted behavior; the new emulator-delivered-`Debug` mechanism does not change it
adversely — if anything it provides an *earlier* trap. `step_one_entry`'s only
post-condition is forward progress (`debug_assert!(icount > 0)`), which still holds.

**Conclusion:** `step_one_entry` does **not** need the land_at-style re-arm and is not
silently broken. However, the *equivalence argument* above is non-obvious and currently
lives only in this review — see suggestion S1 (add a one-line cross-reference comment in
`step_one_entry` so a future editor does not "fix" it by mirroring the land_at re-arm,
which would be wrong: re-arming after `break` is unreachable, and re-arming the `Debug`
arm there has nothing to re-arm because the loop exits). This is a **Suggestion**, not a
blocker, because the code is correct as written.

## Performance — ACCEPTABLE

One extra `KVM_SET_GUEST_DEBUG` ioctl per single-step that hits a `Debug` exit (i.e.
nearly every step, since a successful step *is* a `Debug` exit). Cost analysis:

- Near walks are bounded by `skid_margin + resync_slack` steps. Production default is
  8192 + 1024 = 9216; landing_precision's tight passes use 256/256 and 128/128.
- landing_precision M2 acceptance = 20k landings, each walking ≤ its margin. Worst case
  is ~20k × 9216 extra ioctls, but the real landings walk only their actual skid
  (measured max 31-39), so the *typical* added cost is ~20k × tens of ioctls.
- A `KVM_SET_GUEST_DEBUG` ioctl is cheap relative to a `KVM_RUN` round trip that is
  already paid on every step. The added overhead is a small constant factor on an
  already step-bounded path. Three full suites (incl. the 20k M2) passed per the commit
  message; this review re-ran the new live tests and dh-vmm clippy without timing
  regressions visible. **Acceptable.**

Note: the re-arm fires on every step, not only after MMIO completions, because the host
cannot distinguish an emulator-delivered `Debug` from a hardware `#DB` (the commit
comment states this). Cheaper alternatives (re-arm only after a preceding MMIO exit)
would be fragile and were correctly not attempted.

## Determinism — PRESERVED

`set_singlestep` toggles `KVM_GUESTDBG_ENABLE|SINGLESTEP`, a host-side trap-control
register. It changes **when KVM returns control to the VMM**, never guest register/memory
state and never the retired-instruction count (`exclude_host=1` PMC). Therefore no
landing target can move: the counter read at the loop top is unchanged by re-arming, the
break condition (`c == target`) is unchanged, and the landed `(icount, rip)` identity is
unchanged. landing_precision's cross-boot identity (same targets, different margins,
identical landed tuples) is exactly the property that would break if the re-arm perturbed
guest state — it passed 3× per the commit message. **Determinism preserved.**

## Kernel claims — PLAUSIBLE, appropriately hedged

The commit message and code comment assert (against kernel 6.8):
1. An emulator-delivered `Debug` (singlestep hook on emulated-MMIO completion) consumes
   the `guest_debug` arming.
2. An `immediate_exit`-based single-instruction completion belt does NOT work on 6.8
   because `EINTR` pre-empts `complete_userspace_io`.

Both are consistent with how KVM's x86 emulator interacts with `KVM_GUESTDBG_SINGLESTEP`
and userspace-MMIO completion on the 6.8 series, and — crucially — claim (1) is now
backed by a *measured, reproducible* regression (the reverted-fix experiment in this
review reproduces the overshoot). The empirical anchoring is the right standard for this
codebase. The comment already hedges correctly ("Hardware-delivered #DBs do not need it,
but the two are indistinguishable here"). No change required; see S4 for a one-word
hedge nicety on the kernel-version scoping.
