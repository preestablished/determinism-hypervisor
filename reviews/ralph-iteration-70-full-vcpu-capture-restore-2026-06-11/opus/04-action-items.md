# Action Items

Branch: `ralph/iteration-70-full-vcpu-capture-restore` · Reviewer: Claude Opus ·
2026-06-11 · Verdict: **APPROVE**

All items below are non-blocking polish. Nothing here gates merge.

### Critical

- [ ] None.

### Important

- [ ] None.

### Suggestions

- [ ] **S1 — Fix stale binding version in SAFETY comment.** In
  `crates/dh-vmm/src/vcpu_state.rs:185`, change "no FAM tail in the 0.14 binding"
  to reflect kvm-bindings **0.13.0** (the actual pinned version) and that the
  code writes only `region[..]`, leaving the `__IncompleteArrayField` tail empty
  to match the fixed-size `KVM_SET_XSAVE` ioctl. Technical claim is fine; only the
  version number and FAM description need correcting.

- [ ] **S2 — Pin captured-struct ABI sizes so a kvm-bindings bump fails loudly.**
  Add a `const _: () = assert!(...)` (or a unit test) in `vcpu_state.rs` pinning
  `size_of` for `kvm_regs`/`kvm_sregs`/`kvm_fpu`/`kvm_xcrs`/`kvm_vcpu_events`/
  `kvm_debugregs`. Today an accidental size drift from a dependency update would
  pass `VCPU_SECTION_VERSION == 1` and silently re-frame the section, diverging
  from peers on a different bindings minor. This forces a deliberate
  `VCPU_SECTION_VERSION` bump instead.

- [ ] **S3 — Annotate the two's-complement TSC offset cast.** Add a one-line
  comment at `vcpu_state.rs:208` noting that `vns.wrapping_sub(host_tsc) as i64`
  is an intentional two's-complement reinterpret of the signed KVM offset, not a
  narrowing bug.

- [ ] **S4 — Widen live round-trip perturbation (optional).** In
  `live_get_set_get_roundtrip`, perturb a DEBUGREGS/XCRS-reachable field (e.g.
  `dr7` via `set_debug_regs`) in addition to `regs.rax`/`regs.rip`, to exercise
  more of the SET path before the GET→SET→GET fixed-point assertion. The
  synthetic-state codec test already covers all fields byte-exactly, so this is
  coverage breadth, not a gap.

### Verification performed (for the record)

- [x] `cargo test -p dh-vmm --lib` → **98 passed / 0 failed**, including the 4 new
  `vcpu_state` tests (2 live KVM round-trips on this box's `/dev/kvm`).
- [x] §8.3 ordering checked constraint-by-constraint against the ARCH text —
  satisfied.
- [x] Padding audit of all 6 byte-copied structs against kvm-bindings 0.13.0
  layout asserts — no implicit padding; all reserved/pad fields named and zeroed.
- [x] `RESTORE_MSR_LIST` vs `hash.rs::MSR_CAPTURE_LIST` — identical order,
  IA32_TSC correctly omitted.
- [x] TSC restore matches `docs/decisions/tsc-alignment.md` — offset attribute,
  decision honored not reopened.
- [x] EFER double-set (SREGS + MSR list) — benign (same value, MSRs-last wins).
- [x] XSAVE2 fail-closed guard placement (capture-only) — correct.
