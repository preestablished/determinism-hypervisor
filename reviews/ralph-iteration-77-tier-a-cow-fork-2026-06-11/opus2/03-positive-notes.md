# Positive Notes

1. **Fail-closed seal check is exactly right.** `fork_slot_vm` reads
   `parent.ram_seals()` and refuses an unsealed parent *before* cloning the fd or
   mapping anything (kvm.rs:154–161). The error message names the fix
   (`freeze_ram first`) and the risk (R9). `fork_preconditions_fail_loudly`
   (tests:638–657) proves the kernel guard is independent of the software
   `Frozen` guard — a caller lying about `Frozen` is still caught. This is the
   single most important property of the change and it is implemented and tested
   correctly.

2. **`assemble_slot_vm` extraction is a faithful, order-preserving refactor.** I
   diffed the post-extraction sequence against the original inline body: dirty-ring
   → USER_SPACE_MSR → `madvise_nohugepage` → memslot → vPMU-off → vCPU → CPUID is
   byte-for-byte the same order (kvm.rs:193–261). The `madvise_nohugepage` call
   genuinely runs for the fork path because it lives in the shared assembler, not
   in `create_slot_vm`. No silent reordering crept in.

3. **One codec, two transports is real, not aspirational.** `build_dhsnap`
   (now `pub(crate)`) and the extracted `apply_dhsnap` are the *same* functions a
   tier-B restore uses; the fork path adds zero fork-only serialization. The
   `AppliedMachine` extraction cleanly factors the RAM-independent half out of
   `restore_snapshot` without changing `restore_snapshot`'s public shape or losing
   the `NotPaused` state check (which correctly stayed in `restore_snapshot`,
   restore_engine.rs:122, while `apply_dhsnap` intentionally has none — the fork
   engine owns its own `Frozen` check).

4. **Guest-level CoW isolation is tested end to end on real KVM.**
   `guest_writes_in_the_child_cow_and_never_reach_the_parent` (tests:461) actually
   runs a guest program in the child (three MOVs + HLT), faults pages through the
   private mapping at the EPT level, and verifies each write landed in the child and
   left the frozen parent at zero. This is the §8.4 hot path exercised for real, not
   a host-side `write_slice` stand-in — and the test pairs with
   `..._cow_isolates_host_writes` for both directions.

5. **`second_child_sees_the_pristine_parent_after_first_child_diverged`**
   (tests:533) is the right test to have: it proves the frozen parent is a *stable*
   fork base across siblings — child1 scribbles a full page, child2 forked afterward
   still sees `[0xAB; 64]`. This is the reproducibility property the a6s ACCEPT
   builds on, and catching a regression here (e.g. an accidental `MAP_SHARED`) would
   be immediate.

6. **The entropy-continues-identically design decision is documented at the type.**
   `ForkOutcome.entropy`'s doc (fork_engine.rs:163–167) states plainly that both
   siblings continuing the parent's stream position is *correct* (§5: divergence
   comes from injected inputs, never the fork), pre-empting the "siblings share a
   PRNG, isn't that a bug?" review reflex. The test asserts
   `outcome.entropy.state() == entropy_p.state()` (tests:431), pinning it.
