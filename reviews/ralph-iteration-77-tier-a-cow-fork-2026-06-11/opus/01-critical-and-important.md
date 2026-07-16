# Critical & Important Findings

**None.**

No correctness, soundness, or safety defects were found. Specifically verified:

## R9 dual-guard is coherent and the docs are honest

The engine half (`fork_slot` rejects any `parent_state != SlotState::Frozen`) and the
kernel half (`fork_slot_vm` rejects a memfd without `F_SEAL_FUTURE_WRITE`) are
independent on purpose, and `fork_preconditions_fail_loudly` exercises *both* — including
a "lying caller" that passes `Frozen` over an unsealed memfd and is caught by the kernel
check. The docs in `kvm.rs:135-178`, `kvm.rs:298-325`, `lib.rs:49-55`, and ARCH §8.4 all
state plainly that `F_SEAL_WRITE` is unavailable while the parent's own KVM mapping lives,
so the parent's *existing* mapping stays writable at the kernel level and the
`SlotState::Frozen` software guard is the only thing preventing a parent write from
corrupting every child's shared baseline. No doc overclaims kernel protection. The
parent-unfreeze-while-children-live concern (Frozen→Paused keeps seals) is explicitly
labeled the slot manager's bookkeeping job (R9) in both `fork_engine.rs:170-174` and the
§8.4 prose — honest.

## assemble_slot_vm extraction is byte-for-byte order-preserving

Construction order in `assemble_slot_vm` (kvm.rs:183-276) is identical to the pre-refactor
`create_slot_vm`: `create_vm` → dirty-ring cap (BEFORE any vCPU) → USER_SPACE_MSR →
`madvise_nohugepage` on the **region** → `set_user_memory_region` → vPMU-off cap →
`create_vcpu(0)` → CPUID mask. The only thing hoisted *out* is RAM creation, which now
happens in each caller before the shared tail — exactly the right seam. The madvise and
dirty-ring cap therefore apply to the child too (good: 4 KiB CoW granularity is doubly
load-bearing for forks, and the dirty ring is ready for future incrementals).

## apply_dhsnap extraction is behavior-identical to the tier-B path

Diffed against `HEAD~1`: the extracted body is the prior inline steps 3–6 verbatim. Error
precedence (MCFG → TIME → LAPC → ENTR → device shape checks → device loop → vCPU →
counter/dirty reseed) is unchanged; `counter.reset()` and `dirty.clear()` still fire only
when `Some`; the terminal value changed from `RestoreOutcome { pages_loaded, .. }` to
`AppliedMachine { .. }` with `pages_loaded` reattached by `restore_snapshot`. The
`restore_engine` test suite (5/5) confirms no behavioral drift.

## try_clone / MAP_PRIVATE / NORESERVE are correct

`File::try_clone` yields a new fd referring to the same open file description / inode;
either is fine for `mmap`, and the seals (an inode property) are visible through the
clone, so the child's `ram_seals()` would observe the parent's seals. `MAP_PRIVATE` is the
CoW the design wants; `MAP_NORESERVE` defers commit (its OOM-on-first-touch implication is
noted as a suggestion, not a defect — it is the intended trade for the <10 ms target).
