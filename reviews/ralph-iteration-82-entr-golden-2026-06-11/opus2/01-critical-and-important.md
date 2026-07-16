# Critical and Important Findings

## Critical

None.

I specifically hunted for: HLT re-entry double-execution, lost r8/r9 across restore,
GPA ring collision, MAX_FILL/0xDEAD memory clobber, log ordering/capacity faults, and
dishonest snapshot attestation. All cleared — see 03-positive-notes.md for the reasoning.

---

## Important

### I1. The novel "HLT → re-enter → resume next batch" path is the design's keystone and has *no* direct unit/integration coverage outside this acceptance

**Where:** `tests/nanokernel/asm/entropy_draw.asm:426-427` (`hlt` / `jmp .batch`);
`crates/dh-worker/tests/entr_golden.rs:run_one_batch` (one `run_segment` per batch,
asserting `GuestHalted`).

**What I verified.** The whole batched design rests on this sequence:

1. Guest executes `hlt` with IF=0 (no STI before it). KVM emulates `hlt`, **advances RIP
   past it**, and exits to userspace with `VcpuExit::Hlt`.
2. `run_segment`'s exit wrapper (runctl.rs:241) sets `halted=true` and unwinds to
   `finish_halted`, which reads the real icount and `get_regs().rip` (now pointing at
   `jmp .batch`) and returns `StopReason::GuestHalted`.
3. The **next** `run_one_batch` calls `run_segment` again on the same slot. KVM_RUN
   resumes at the post-`hlt` RIP (`jmp .batch`), i.e. it does **not** re-execute `hlt`.

This is correct KVM behavior. **But** the existing test base does not exercise step 3
anywhere: `pad_echo` loops with `jmp .frame` and never halts; `pipeline_smoke`
(`terminal_hlt_is_a_stop_not_a_fault_live`, runctl.rs:665) halts **exactly once** and the
test stops — it never re-enters after a HLT. The `boundary.rs` docstring (lines 205-209)
even warns that an `Ok`-handled Hlt under `step_one_entry` would "re-run and silently
chain MULTIPLE logical entries under one step." So the re-entry-resumes-past-hlt contract
is real, is documented as load-bearing, and yet this acceptance is the *first and only*
thing that depends on it — and it depends on it 2 (leg A pre-snap) + 4 (leg A golden) +
4 (leg B) = 10 times per run.

**Why it matters.** If KVM ever exits Hlt with RIP *at* the `hlt` (e.g. a kernel/arch
variant, or a `KVM_CAP` that changes skip-emulation behavior), the next segment would
re-execute `hlt` and immediately re-halt — the guest would make **zero** forward progress,
every batch would draw 0, and the failure mode would be a confusing "count_pause is short"
assert with no signpost toward the real cause (HLT-skip semantics). The acceptance would
catch it, but only as a downstream count mismatch, not as a localized diagnostic.

**Recommendation.** Add a *focused* live regression in `dh-vmm`'s runctl tests (alongside
`terminal_hlt_is_a_stop_not_a_fault_live`): a tiny guest that HLTs, then on resume bumps a
guest-RAM counter and HLTs again; assert two consecutive `run_segment` calls each return
`GuestHalted` **and** that the counter incremented between them (proving RIP advanced past
`hlt`, not re-executed it). This pins the keystone contract at the VMM layer where it
belongs, independent of the entire entropy/snapshot stack, and turns a future regression
into a one-line diagnostic instead of a forensic dig through the golden test.

This is the only finding above "suggestion." It is **not** a blocker — the code is correct
on Linux/kvm-intel today; it is a coverage and diagnosability gap on the exact behavior the
iteration newly relies on.
