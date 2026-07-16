# Action Items

Self-contained list. Nothing here blocks merge of
`ralph/iteration-30-cpuid-determinism-mask`.

### Critical

_None._

### Important

- [ ] **Config bead: reconcile the two CPUID-table hashes and the `flags` field.**
  This iteration adds `cpuid.rs::cpuid_table_hash()` (hashes 7 u32: function, index,
  **flags**, eax..edx). `config.rs` already has `MachineConfig.cpuid_table:
  Vec<CpuidLeaf>` and `canonical_encode()` hashes 6 u32 (function, index, eax..edx —
  **no flags**) into the `machine_config_hash` preimage (H_0, §8.5). When the config
  bead wires the masked KVM table into `MachineConfig.cpuid_table`, deliberately
  decide: either add `flags` to `CpuidLeaf`/the preimage, OR document that `flags` is
  intentionally excluded from machine-config identity (it is a KVM indexing hint, not
  guest-visible state) and note in `cpuid.rs` why `cpuid_table_hash` includes it (it
  hashes the raw KVM table for the diff tool, not the config identity). Not a defect
  today — `cpuid_table_hash` is only consumed by `cpuid-diff`. See 01-…md §I1.

### Suggestions

- [ ] **Clear `WAITPKG` (CPUID.7.0:ECX[5])** in the `(7,0)` ECX arm — `UMWAIT`/`TPAUSE`
  are host-TSC-deadline user-mode wait primitives, same class as the already-cleared
  MONITOR/TSC_DEADLINE. Not advertised on the Coffee Lake lab host (so harmless here),
  but a forward-fleet hardening for newer Intel parts. See 02-…md §S1.
- [ ] **Consider clearing `TSC_ADJUST` (CPUID.7.0:EBX[1])** so the advertised feature
  set stays consistent with the default-deny MSR filter on `IA32_TSC_ADJUST`. See §S2.
- [ ] **Fix `Vec::with_capacity` hint** in `cpuid_table_hash`: `entries.len() * 24`
  should be `* 28` (7 u32 = 28 bytes/entry now that `flags` is included). Cosmetic. §S3.
- [ ] **`cpuid-diff`: print/assert "masked-only entries: 0"** so a future regression
  that adds a leaf is visible instead of silently dropped (the tool only walks the
  supported→masked direction). §S4.
- [ ] **Centralize the `hex()` helper** in `tools/dh-cli/src/cpuid.rs` if a shared hash-
  rendering util already exists elsewhere in dh-cli/dh-inputlog. §S5.

### Verified (no action)

- All 10 cleared bit constants match Intel SDM bit assignments (table in 00-overview).
- Mask covers everything §7.2 *requires*; AVX512 LCD masking is the only §7.2 item
  unimplemented and it is explicitly optional / determinism-class, not correctness.
- 45 dh-vmm/dh-cli tests pass live; clippy clean; live `cpuid-diff` sane.
- vPMU-off cap correctly enabled before `create_vcpu`; best-effort acceptable given
  leaf 0xA is zeroed unconditionally.
- `KVM_SET_CPUID2` placement (after `create_vcpu`, before any `KVM_RUN`) is correct.
- hello/landing_loop guests don't execute CPUID — no guest-visible regression risk.
