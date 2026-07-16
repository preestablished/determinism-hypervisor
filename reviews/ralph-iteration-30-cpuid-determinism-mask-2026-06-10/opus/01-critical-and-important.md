# Critical and Important Findings

## Critical

**None.** No correctness, determinism, or safety defects found. Every masked bit is
SDM-correct, the mask starts from `KVM_GET_SUPPORTED_CPUID` (never host CPUID), the
masked table reaches the vCPU via `KVM_SET_CPUID2` before any `KVM_RUN`, and the live
tests confirm the running vCPU carries the masked table.

---

## Important

### I1 — Two independent hashes of the CPUID table; `flags` field treated differently (consistency, deferred to config bead)

The bead text says *"Masked table goes into MachineConfig and is hashed."* This
iteration provides `cpuid_table_hash()` but does **not** wire the masked table into
`config.rs::MachineConfig`. The prompt asked whether that wiring is in-scope or the
config bead's job. **Verdict: correctly the config bead's job — but flag a latent
inconsistency the config bead must resolve.**

`crates/dh-vmm/src/config.rs` already has the slot and the preimage for this:

```rust
pub struct MachineConfig {
    ...
    pub cpuid_table: Vec<CpuidLeaf>,   // line 84
    ...
}
```

and `canonical_encode()` (lines 191-203) already serializes each `CpuidLeaf` into the
`machine_config_hash` preimage (which feeds H_0 of the state-hash chain, §8.5). So
there are now **two** distinct canonical hashes of the same logical CPUID table:

1. `config.rs::CpuidLeaf` encoding: 6 u32 fields — `function, index, eax, ebx, ecx,
   edx`. **No `flags` field** (`CpuidLeaf` doesn't carry it).
2. `cpuid.rs::cpuid_table_hash()`: 7 u32 fields — `function, index, **flags**, eax,
   ebx, ecx, edx`.

This is not a bug in *this* PR (the two are used for different purposes today: the
config preimage is the H_0 input; `cpuid_table_hash` only backs the `cpuid-diff`
acceptance dump). But when the config bead wires the masked KVM table into
`MachineConfig.cpuid_table`, it must decide deliberately:

- The conversion `kvm_cpuid_entry2 -> CpuidLeaf` **drops `flags`**
  (`KVM_CPUID_FLAG_SIGNIFICANT_INDEX`, etc.). Two leaves with identical
  (function,index,eax..edx) but different `flags` would collide in the config hash but
  not in `cpuid_table_hash`. In practice `flags` is derived from the leaf identity, so
  this is benign — but it should be a *recorded decision*, not an accident of two code
  paths that happen to disagree.
- Recommendation for the config bead: either (a) add `flags` to `CpuidLeaf` and the
  config preimage so the two hashes are reconcilable, or (b) document explicitly in
  config.rs that `flags` is intentionally excluded from machine-config identity (it is
  a KVM indexing hint, not guest-visible state) and add a note in `cpuid.rs` that
  `cpuid_table_hash` includes `flags` only because it hashes the *raw KVM table* for
  the diff tool, not the MachineConfig identity.

Either way: **this iteration is correct to stop at the hash function**; the wiring and
the flags reconciliation belong to the config bead. Flagged as Important so the config
bead inherits this explicitly rather than rediscovering it.

**Severity rationale:** Important, not Critical, because nothing in the current build
*depends* on the two hashes agreeing — `cpuid_table_hash` is only consumed by the
diagnostic `cpuid-diff` subcommand today. No determinism guarantee is currently
violated.
