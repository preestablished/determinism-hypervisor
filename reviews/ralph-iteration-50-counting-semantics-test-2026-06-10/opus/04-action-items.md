# Action items

### Critical

_None._

### Important

_None._ (Verdict is APPROVE; merge is not blocked.)

### Suggestions

1. **Record the microcode revision in the R2 class empirics.** This box: kernel
   `6.8.0-124-generic`, microcode `0xfa`. Add the microcode next to the "kvm-intel class"
   note on `COUNTING_DELTA_AT_OUT_EXITS` in `tests/nanokernel/src/lib.rs` (and/or the
   trap-eating comments in `crates/dh-vmm/src/boundary.rs`), so the per-class empiric is
   pinned to (kernel, microcode) and the future R2 alarm has a concrete baseline.

2. **Promote the MMIO-write sentinel to a structured error variant.** In
   `tests/determinism/tests/counting_semantics.rs`, the step-ending signal is
   `BoundaryError::Exit("mmio-write-ends-step")` matched by string equality. Correct and
   unambiguous today, but a dedicated variant (e.g. `BoundaryError::MmioWriteStep`) would be
   self-documenting and copy-paste-proof. Test-ergonomics only; do not block on it.

3. **Reconcile the "~700 instructions" free-run figure.** The doc-comment on
   `landing_across_an_mmio_write_does_not_free_run` says ~700; the measured un-re-armed
   free-run from the MMIO write is 991 instructions (fix-reverted Overshoot reported
   `counted: 1003`). Either tighten the number or drop it to avoid misleading a future
   reproducer. Cosmetic.
