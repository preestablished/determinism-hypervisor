# Action Items

Self-contained follow-ups. None block merge of iteration 82.

### Critical

None.

### Important

- [ ] **Pin the HLT→resume-across-segments contract at the VMM layer.** Add a focused live
  regression in `crates/dh-vmm/src/runctl.rs` tests (next to
  `terminal_hlt_is_a_stop_not_a_fault_live`, ~line 665): a minimal guest that `hlt`s, then
  on resume increments a u64 in guest RAM and `hlt`s again. Drive two consecutive
  `run_segment(Until::IcountBudget(...))` calls on the same slot. Assert **both** return
  `StopReason::GuestHalted` **and** that the guest-RAM counter went from 0 to 1 between
  them. This proves KVM resumes *past* `hlt` (RIP advanced, not re-executed) — the exact
  property the entire `entr_golden` batched design depends on, currently exercised nowhere
  else (pad_echo loops without halting; the existing terminal-HLT test halts once and
  stops). Without this, a future KVM/arch change to HLT-skip semantics surfaces only as an
  opaque "count short" failure in the golden test. File as a bd issue, label `testing`,
  priority P1. (Detail in `01-critical-and-important.md` §I1.)

### Suggestions

- [ ] **Assert the snapshot boundary icount survives restore.** In `entr_golden.rs` after
  the `restore_snapshot` call (~line 307), add
  `assert_eq!(outcome.cumulative_icount, a1.boundary.icount);`. The field is already
  bound but unread; it equals `a1.boundary.icount` by construction
  (`snapshot_engine.rs:232` → `restore_engine.rs:356`) and strengthens the round-trip claim
  to cover §3.1 accounting, not just the entropy tuple. Zero cost. (`02-suggestions.md`
  §S1.)

- [ ] **Clarify or drop the `.fault` LEN poison.** In `entropy_draw.asm:430`, either remove
  `mov dword [r8+REG_LEN], 0xDEAD` (the un-bumped `count` already trips the harness's
  exact-count assert at `entr_golden.rs:254`) or keep it with a comment that it is a
  human-debug marker for memory dumps only — **not** a harness-checked signal. The current
  comment "poison: harness count check trips" overstates its role: the count *shortfall*
  trips the harness, the LEN write is inert (no subsequent doorbell reads it; and note
  `0xDEAD = 57005 < MAX_FILL = 1<<20`, so a hypothetical retry would *succeed* with a 57 KB
  fill, a latent footgun for any future fault-then-retry guest — flag in the comment).
  (`02-suggestions.md` §S2.)

- [ ] **(No-op, recorded for trail) Cross-crate test-helper duplication is acceptable.**
  `VmMem`, `fresh_log`, `config` are duplicated between `entr_golden.rs` (dh-worker) and
  `m1_acceptance.rs` (determinism). Different crates → sharing isn't worth a published
  helper crate. **Do not** hoist `VmMem` into `dh-worker/tests/common/mod.rs` (no other
  dh-worker test needs it). If a *third* dh-worker test later needs `fresh_log`/`config`,
  promote those two (dh-worker-local) into `common/mod.rs` then. (`02-suggestions.md`
  §S4/S5.)

---

## Note on bead determinism-hypervisor-4a3 (landing vs MMIO-write trap loss)

Reviewed via `bd show`. The filing is **actionable and well-scoped**:

- **Repro is concrete:** "revert `entr_golden.rs` to `Until::Goal{poll_period:4096}` on the
  entropy_draw guest" — a one-line change that reproduces the overshoot (target 4096,
  counted 4170, ~74 instrs over).
- **Hypotheses are listed:** isolate *which* MMIO write shape loses the re-armed trap
  (width? imm-to-mem vs reg? back-to-back MMIO? the device's host-side guest-RAM write
  during the exit), then a `boundary.rs` fix (candidate: complete the instruction under a
  1-instruction counter-arm backstop after servicing an MmioWrite while stepping).
- **Scope/impact is stated:** affects goal polling and pause roll-forward for any
  MMIO-dense guest (M5+); dy8 itself dodges it via HLT batch boundaries.

This correctly cross-references the iteration-50 fix (boundary.rs:168-176 re-asserts
guest_debug after a non-Debug exit) and notes that at least one eaten trap still escapes
with this guest's MMIO mix. The only refinement I'd add to the bead: name the candidate
backstop more precisely — after an `MmioWrite` exit under stepping, arm the PMI period to 1
(instead of `NEVER_FIRES_PERIOD`) as a fallback so the next retirement still traps even if
TF was cleared by the emulator. Priority P1 is appropriate given it gates M5+ device-driven
landings. No changes needed to the bead to start work.
