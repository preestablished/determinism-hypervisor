# Action Items

### Critical
_None._

### Important
_None._

### Suggestions
- [ ] [tests/nanokernel/src/lib.rs:130] Hoist the "kvm-intel-class only" caveat from the doc prose into the constant name or its first doc line (e.g. note AMD/other KVM may differ), so bead gfb cannot silently trust 997 cross-vendor. The honesty is already in the comment; make it un-missable.
- [ ] [tests/nanokernel/asm/counting.asm:11-16] Add an assembly-time guard for the exiting-instruction count: an `%assign EXITCOUNT 0` bumped by an `XI` macro wrapping CPUID / MMIO-read / MMIO-write, with `%if EXITCOUNT != 3 %error`, mirroring the existing `%if ICOUNT != 1000` guard, so `COUNTING_EXIT_INSTRS_IN_REGION = 3` (lib.rs:113) is build-enforced rather than hand-maintained.
- [ ] [tests/nanokernel/asm/counting.asm:91] Add a one-line comment that the 'E' marker's `mov dx` / `mov al` retire INSIDE the window and only the OUT is excluded — the precise count consequence, for the next editor.
- [ ] [tests/determinism/tests/counting_smoke.rs:154] Optionally also assert the absolute S counter value (s == 6 on this box) to catch crt0/prologue instruction-count drift that shifts both endpoints together while leaving the delta misleading.
- [ ] [tests/determinism/tests/zz_scratch_counting_probe.rs] Delete the leftover untracked reviewer scratch file from a prior session (already removed during this review's cleanup; flagged so the author is aware it existed). Verify `git status` is clean of `zz*`/scratch debris.
