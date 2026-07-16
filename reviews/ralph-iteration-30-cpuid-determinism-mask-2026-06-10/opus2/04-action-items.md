# Action items

Self-contained follow-ups. None block merge of iteration 30.

### Critical

_None._

### Important

- [ ] **Clear WAITPKG (leaf 7.0 ECX[5]) in `mask_in_place`.** Add `const L7_ECX_WAITPKG: u32 = 1 << 5;` (comment: "UMWAIT/TPAUSE — host-timed wall-clock wait, MWAIT-class") and extend the `(7, 0)` arm with `e.ecx &= !L7_ECX_WAITPKG;`. Add the matching assertion to the live mask test. Not visible on the Coffee Lake lab box (leaf 7.0 ECX bit5 is clear there), but advertised straight through on any Tiger-Lake-or-later host the fleet might add. `crates/dh-vmm/src/cpuid.rs`. (I-1)

- [ ] **Decide and implement the policy for host-specific frequency/topology leaves 0x15, 0x16, 0x1A.** Either (a) zero them in `mask_in_place` so the masked table and `cpuid_table_hash` are host-SKU-independent (matches "no host-specific values in the table"), or (b) explicitly document in ARCH §7.2 that they are accepted host-passthrough and the determinism class is established by the pinned-kernel/microcode tuple (§7.4), not CPUID. On this box they are KVM-zero so the hash is incidentally clean; on hosts where KVM populates 0x16 they would split the machine-config hash across SKUs. Note 0x1A specifically matters for **restore on a different host**. `crates/dh-vmm/src/cpuid.rs` + `.agents/docs/.../ARCHITECTURE.md §7.2`. (I-2)

- [ ] **Make `KVM_PMU_CAP_DISABLE` fail loud (or at least warn).** Preferred: add `(KVM_CAP_PMU_CAPABILITY, "KVM_CAP_PMU_CAPABILITY")` to `REQUIRED_RAW_CAPS` and change `let _ = vm.enable_cap(&pmu_cap);` to `?`-propagate `KvmError::VmCreate`. Cap exists since kernel 5.18; this is a 6.8 pinned-kernel fleet, so the defensive swallow buys nothing while diverging from the REQUIRED_CAPS hard-fail discipline. If best-effort is deliberate, emit `tracing::warn!` on `Err` and document the residual (guest-vPMU vs host INST_RETIRED contention, §3.1) as verification-mode-monitored. `crates/dh-vmm/src/kvm.rs`. (I-3)

### Suggestions

- [ ] **Add an off-box synthetic unit test** with `CpuId::from_entries(&[...])` covering mask bits, full-zero leaves (6, 0xA), PV-leaf `retain` (assert `nent` shrinks), and hash order-independence + one-bit sensitivity — so the module has coverage on lanes without `/dev/kvm` (currently all 3 tests skip). Highest-value follow-up. `crates/dh-vmm/src/cpuid.rs`. (S-1)

- [ ] **Drop the "RDRAND on every host" assumption** in the live mask-sensitivity test; assert a KVM-universal expected change (PV leaves removed) instead. `crates/dh-vmm/src/cpuid.rs`. (S-2)

- [ ] **Annotate `cpuid-diff` cleared masks with bit names** (reuse the `L*` constant meanings) for M1 acceptance-review readability; keep numeric form too. `tools/dh-cli/src/cpuid.rs`. (S-3)

- [ ] **Prepend `entries.len() as u32` to the hash preimage** as cheap framing/domain-separation hardening. `crates/dh-vmm/src/cpuid.rs`. (S-4)

- [ ] **Comment the `(7, 0)` arm** to state that leaf-7 subleaves ≥1 carry no determinism-class bits today and to revisit when AVX512/AMX masking lands (§7.2 "lowest common denominator"). `crates/dh-vmm/src/cpuid.rs`. (S-5)

### Notes for the next reviewer / maintainer

- Verified live on Intel i5-8400 (Coffee Lake), kernel 6.8.0-124, `/dev/kvm` rw. `cargo test -p dh-vmm cpuid` = 3/3 green; `dh-cli cpuid-diff` runs. Raw `KVM_GET_SUPPORTED_CPUID` dump confirmed: 42 entries, **no duplicate `(function,index)` pairs** (so the `sort_by_key` stability concern is theoretical on this host); leaves 0x15/0x16 present-but-zero; no leaf 0x1A (non-hybrid SKU); leaf 7.0 ECX = `0x4` (no WAITPKG).
- The duplicate-key hash hazard (two entries sharing `(function,index)` differing only in `flags` → `sort_by_key` keeps input order → order-dependence) did not reproduce on this box and is low-likelihood; not raised as a finding, but if a future host's table ever shows dups, the hash's order-independence guarantee would need the sort key to include `flags`.
- KVM out-of-range CPUID behavior after removing 0x40000000 is deterministic **per pinned kernel** (KVM returns max-basic-leaf values for out-of-table leaves below the hypervisor range; the table is fixed and hashed, so the returned value reproduces). Acceptable given §7.4 pins the kernel; worth a one-line note in §7.2 if cross-kernel identity is ever required.
