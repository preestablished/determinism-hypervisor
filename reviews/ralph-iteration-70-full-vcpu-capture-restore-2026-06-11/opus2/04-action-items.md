# Action Items

Self-contained checklist distilled from this review. Severity tags map to
`01-critical-and-important.md` (I1, I2) and `02-suggestions.md` (S1–S4).

## Critical

- [ ] None. No merge-blocking defect exists inside `vcpu_state.rs`. The two
      Important items below are spec/integration reconciliations the qmp/9e4
      bead inherits, not bugs in this branch's code.

## Important

- [ ] **(I1) Reconcile the two VCPU-section encoders before wiring qmp/9e4.**
      `crates/dh-vmm/src/vcpu_state.rs::encode_section` (raw repr(C) byte-copy,
      14-entry MSR list, padding included) is NOT byte-identical to
      `crates/dh-vmm/src/hash.rs::canonical_vcpu_blob` (field-selective,
      15-entry MSR list with normalized IA32_TSC inserted at slot 13, padding
      excluded), yet `crates/dh-snapshot/src/dhsnap.rs:15` documents
      `canonical_vcpu_blob` as *the* VCPU-section encoder. Decide which byte
      stream the DHSNAP `VCPU` section actually carries, ensure the snapshot-ref
      hash is computed over *that* stream, and either unify the two encoders or
      add a doc + pinning test that records the intentional differences (MSR
      count 14 vs 15; padding in/out). Until reconciled, "snapshot hash =
      H(section bytes)" is false.

- [ ] **(I2) Reconcile ARCH §4 defense-4 ("align guest TSC on every VM entry")
      with `docs/decisions/tsc-alignment.md` (once-per-restore offset) and with
      the code.** `vcpu_state.rs:187-189` sets the TSC offset once from a
      userspace `rdtsc()`; there is NO per-entry realignment in `run.rs`/
      `runctl.rs`. Either (a) delete "every VM entry" from ARCH §4 and declare
      once-per-restore-with-intended-drift the contract, or (b) wire a per-entry
      `set_tsc_offset` into the run loop. Also note the unbounded
      `rdtsc()`→`KVM_RUN` scheduling skew: if resume-precision matters, derive
      the offset from the KVM-visible guest TSC rather than a userspace
      `rdtsc()` snapshot. Minimum action for THIS bead: a doc line in `restore`
      stating per-entry alignment is out of scope / the run loop's job, so no
      reader assumes `restore` satisfies §4 defense-4.

## Suggestions

- [ ] **(S1)** Padding is empirically zero on kernel 6.8 (KVM `GET_*` zeros
      reserved fields), but `encode_section` byte-copies it and that copy feeds
      byte-equality + the cross-fork compare. Add a live unit test asserting the
      known reserved/padding ranges (segment pads, dtable padding[3], fpu
      pad1/pad2, xcrs padding[16] + unused entries, events.reserved[26],
      dbg.reserved[9]) are zero post-capture — so a future kernel that surfaces
      a bit there trips CI instead of silently corrupting equality. (Or zero
      them in `encode_section`, mirroring the XSAVE canonicalization stance.)

- [ ] **(S2)** Fix stale version citations in the module doc: this tree uses
      **kvm-bindings 0.13.0**, not "0.14"/"kvm-ioctls 0.24" as the doc says
      (`vcpu_state.rs:17,37,165`). The "no FAM tail in the 0.14 binding" SAFETY
      note should reference the version that actually compiles.

- [ ] **(S3)** Promote a trimmed version of the post-execution fixed-point
      experiment into the committed suite. The committed `live_get_set_get_
      roundtrip` never calls `vcpu.run()`; running the real-mode harness
      (`out 0xD3,al ; hlt`, reused from `kvm.rs:675`) and then asserting the
      capture→restore→capture fixed point is the stronger property and is
      already proven green on a `/dev/kvm` box.

- [ ] **(S4)** Document `restore`'s partial-mutation failure contract: a
      mid-sequence `KVM_SET_*` error leaves the vCPU half-restored; state that
      callers must discard the slot rather than retry in place. Doc-only, no
      behavior change.

## Verification performed by this reviewer

- [x] `cargo test -p dh-vmm --lib` — 101 pass.
- [x] `cargo clippy -p dh-vmm --lib --all-features` — clean.
- [x] Live padding audit, cross-VM byte-identity, post-execution fixed point,
      EFER double-set — all confirmed on `/dev/kvm` (kernel 6.8).
- [x] Scratch experiments reverted — `git status` clean (verified).
