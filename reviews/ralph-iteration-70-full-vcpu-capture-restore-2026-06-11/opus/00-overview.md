# Review Overview — Full vCPU Capture/Restore (bead 55f)

- **Branch:** `ralph/iteration-70-full-vcpu-capture-restore`
- **Base:** `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Diff:** `/tmp/ralph70-diff.txt` (481 insertions across 2 files)
- **Primary file:** `crates/dh-vmm/src/vcpu_state.rs` (new, 479 lines)
- **Other:** `crates/dh-vmm/src/lib.rs` (+2: module registration)

## Summary

This iteration adds the full KVM vCPU GET/SET round-trip behind the DHSNAP
`VCPU` section codec. `capture()` gathers REGS / SREGS / FPU / canonical XSAVE
(via `crate::xsave` from iterations 68–69) / XCRS / an explicit MSR list /
VCPU_EVENTS / DEBUGREGS, with a fail-closed `KVM_CAP_XSAVE2` size guard that
errors rather than silently truncating on AMX-class hosts. `restore()` follows
ARCHITECTURE §8.3's normative ordering exactly (SREGS→REGS→…→VCPU_EVENTS;
XCRS-before-XSAVE; MSRs last) and programs the guest TSC through the
`KVM_VCPU_TSC_OFFSET` device attribute per `docs/decisions/tsc-alignment.md` —
the decided mechanism is honored, not reopened. The unsafe struct byte-copy
codec was the highest-risk surface; I verified all six kvm-bindings structs
(`kvm_regs`, `kvm_sregs`, `kvm_fpu`, `kvm_xcrs`, `kvm_vcpu_events`,
`kvm_debugregs`) are `#[repr(C)]` plain-old-data with **no compiler-inserted
implicit padding** — every reserved/padding region is a *named* field that
`Default` zeroes and that the kernel zero-fills on GET, so the byte-hashed
encoding is deterministic. The MSR restore list is byte-for-byte order-identical
to `hash.rs`'s capture list and correctly omits IA32_TSC. `cargo test -p dh-vmm
--lib` passes 98/98, including the four new `vcpu_state` tests (two live KVM
round-trips on this box's `/dev/kvm`).

## Verdict

**APPROVE**

No correctness or determinism defects found. The padding-leak class of bug that
iteration 69 fixed for XSAVE does not recur here: the structs are clean by
construction and I checked each one against the kvm-bindings 0.13 layout
asserts. Findings below are non-blocking polish only.

## Stats

| Metric | Value |
|---|---|
| Critical findings | 0 |
| Important findings | 0 |
| Suggestions | 4 |
| Positive notes | 6 |
| Tests | 98 passed / 0 failed (incl. 2 live KVM round-trips) |
| §8.3 ordering | satisfied (verified constraint-by-constraint) |
| TSC decision | honored (offset attribute, not reopened) |
| Struct padding audit | clean (6/6 structs, no implicit padding) |
| MSR list cross-check | identical order to `hash.rs`, IA32_TSC correctly omitted |
