# Critical & Important

**No Critical or Important findings.** The change is correct and the acceptance
is fully discharged. This file records the load-bearing verifications I ran, so
the next reviewer doesn't have to re-derive them.

---

## VERIFIED: resume-after-HLT semantics are sound, not accidental (the highest-value check)

The novel thing this test does is **re-run a segment after a `GuestHalted`
boundary** — every prior guest treats HLT as terminal (m1_acceptance returns
on `GuestHalted`; `crt0.asm`'s `.park: hlt; jmp .park` is the canonical terminal
park). I traced the full chain and it is sound:

1. **The guest runs with IF=0 the entire time.** `dh-vmm/src/boot.rs:249` sets
   `regs.rflags = 2` (only the reserved bit-1; IF/bit-9 is clear). On real
   hardware, `HLT` with IF=0 halts forever (no interrupt can wake it). **Under
   VMX/KVM this does not matter:** the HLT-exiting VM-execution control causes a
   `KVM_EXIT_HLT` regardless of IF, so the hypervisor always regains control.
   `run_segment`'s `exits!` macro (`runctl.rs:241`) catches `VcpuExit::Hlt`,
   sets `halted = true`, and unwinds to `finish_halted`.

2. **RIP is captured one instruction past the HLT.** `finish_halted`
   (`runctl.rs:428-437`) does `KVM_GET_REGS` and stores `regs.rip`. KVM advances
   RIP past `HLT` before reporting the exit, so the captured RIP is the address
   of `jmp .batch`. On the next `KVM_RUN` (next batch's `run_segment`), the guest
   resumes there. This is exactly why `crt0`'s `.park` loop works at all — the
   after-HLT RIP advance is well-established at the KVM layer for every guest;
   this test is merely the first to *re-enter* rather than stop.

3. **Re-entering a segment on the same slot is established.** `m4_transparency`'s
   `run_more` runs back-to-back `run_segment` calls on one slot (stopping on
   `BudgetReached`). Combined with (2), re-entry after a HLT boundary is the
   intersection of two already-exercised behaviors, not new KVM territory.

4. **Empirical confirmation.** The test passes live (it is the acceptance), and
   `read_count` advancing to exactly `(BEFORE+GOLDEN)*BATCH` on both legs proves
   the guest resumed and drew, not re-ran from `prog_main` (which would have
   re-zeroed nothing but would have re-programmed LEN — see leg-B LEN
   dependency below). The semantics are sound; the module docs in both the guest
   and the test record the batch-boundary rationale.

**Conclusion:** sound and adequately documented. No action needed; this is noted
only because the prompt flagged it as the highest-value question.

---

## VERIFIED: both ENTR v2 halves are load-bearing; no false-pass path

- `outcome.entropy` (`restore_engine.rs:196-197`, `apply_dhsnap` →
  `DetEntropy::restore` from the ENTR v2 PRNG half) seeds leg B's PRNG. If broken,
  leg B's draw bytes diverge from `golden` → `assert_eq!(replayed, golden)` fails.
- `bus_b`'s `PvEntropy` regs come from the ENTR v2 **device** half
  (`restore_engine.rs:295-308`, `dev.restore(&entr.device_regs(), 1)`).
  **This is genuinely load-bearing for byte equality** because leg B resumes at
  `.batch`, *after* the one-time `mov [r8 + REG_LEN], eax` at `prog_main`. The
  guest never re-programs `LEN` on the restored path; the device must already
  hold `LEN=16` from the restored regs, or every leg-B draw would be a 0-byte
  fill and `replayed != golden`. So the device-reg restore is exercised by the
  byte-equality assertion, not merely by a register snapshot unit test.
- There is no path by which the device could "re-derive" PRNG state from
  anything other than the restored `DetEntropy` (`PvEntropy::doorbell` draws only
  from `ctx.entropy`, entropy.rs:128). A broken round-trip cannot be masked.

---

## VERIFIED: register liveness across MMIO exits (r13 batch counter)

KVM preserves guest GP registers across MMIO exits; the host `on_exit` only
touches the bus device and guest RAM, never guest GP regs. The batch counter
`r13d`, the bases `r8`/`r9`, and the draw temporaries `rcx`/`rdx` all survive the
`MmioWrite`/`MmioRead` exits unmodified. The pace loop clobbers only
`rax`/`rbx`/`r11`/`r12`, none of which carry state across draws. r13 is safe.
