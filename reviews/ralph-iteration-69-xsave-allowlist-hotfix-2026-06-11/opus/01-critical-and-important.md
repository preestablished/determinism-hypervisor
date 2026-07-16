# Critical and Important Findings

## Critical

**None.** The hash-only consumer (`crates/dh-vmm/src/hash.rs:278`) is correct, and the SDM
correctness of `is_x87_init` and the allowlist rebuild checks out (see 03-positive-notes.md for
the verification details). No data-loss or state-corruption path exists on the *current* call
site.

---

## Important

### I1 — Generic extended-bit init normalization is unsafe for non-all-zero-init components; restrict or gate before 55f reuses this on SET_XSAVE

- **Severity:** Important (latent; safe today because Phase 1 masks all extended components, but the code is generic and the function doc explicitly promises 55f restore reuse)
- **Location:** `crates/dh-vmm/src/xsave.rs:122-128`

```rust
if bv & (1u64 << c.bit) != 0 {
    if area[c.offset..end].iter().all(|&b| b == 0) {
        canon_bv &= !(1u64 << c.bit); // all-zero ⇒ init for bits ≥ 2
    } else {
        keep.push((c.offset, end));
    }
}
```

The comment `all-zero ⇒ init for bits ≥ 2` is **not universally true**. It holds for the
components Phase 1 could ever see (SSE/AVX: YMM_Hi init = zeros), but the loop is generic over
*any* `XsaveComponent` from `host_component_layout()`. Counter-examples that exist in the XSAVE
architecture:

- **PKRU (bit 9):** the architectural init value of the PKRU register is `0x0000_0000` *by the
  current SDM*, so it happens to be all-zero — but PKRU is a 4-byte component padded to 8, and a
  guest that legitimately set PKRU=0 is logically *init* anyway, so clearing the bit is benign
  there. The risk is the **next** component, not PKRU specifically.
- **The general hazard:** any present or future XCR0 component whose architectural init state is
  NOT all-zero (the architecture does not promise zero-init for all components) would be
  *wrongly normalized*: an all-zero area is treated as init and the bit is cleared. On the
  **hash path** that is merely a (vanishingly unlikely) hash collision between two distinct
  logical states. On the **55f SET_XSAVE restore path** it is **state corruption**: XRSTOR with
  `RFBM[i]=1, XSTATE_BV[i]=0` loads the component's *init* state, which would differ from the
  guest's real (all-zero-area-but-non-init-encoded) state.

Because the bit-0 and bit-1 normalizations are hardcoded to the two components whose init
*is* known (x87 via `is_x87_init`, SSE via all-zero XMMs — both correct), the safe move is to
make the extended loop equally explicit rather than heuristic.

**Recommended fix (pick one, document the choice):**

```rust
// Option A — restrict extended init-normalization to a known-safe allowlist of
// components whose architectural init state is all-zero (currently AVX bit 2).
const ZERO_INIT_BITS: u64 = 1 << 2; // YMM_Hi; extend only with SDM citation
if bv & (1u64 << c.bit) != 0 {
    let is_init = (ZERO_INIT_BITS & (1u64 << c.bit)) != 0
        && area[c.offset..end].iter().all(|&b| b == 0);
    if is_init { canon_bv &= !(1u64 << c.bit); }
    else { keep.push((c.offset, end)); }
}
```

or **Option B** — keep the heuristic for the hash path but add a `normalize_init: bool`
parameter (or a separate `canonicalize_for_restore`) so 55f opts *out* of extended-bit
normalization until each component's init pattern is verified. Either way, **block 55f from
reusing the unrestricted form on SET_XSAVE** — file a bead and reference it in the doc.

### I2 — Document the SET_XSAVE/MXCSR safety contract precisely on `canonicalize` for the 55f restore reuse

- **Severity:** Important (documentation/contract; prevents a future corruption bug)
- **Location:** `crates/dh-vmm/src/xsave.rs:58-81` (doc comment) and `hash.rs:274` (the 55f note)

The function doc states the 55f DHSNAP vCPU section reuses this transform and (per
`ARCHITECTURE.md:651`) "Restore feeds the canonical form to `KVM_SET_XSAVE`." The MXCSR
interaction the hotfix gets *right* but does not *document* is load-bearing and must be pinned so
55f does not regress it:

- **SSE bit normalization + MXCSR:** when this code clears XSTATE_BV bit 1 (XMMs all-zero), it
  **keeps** MXCSR/MXCSR_MASK `[24,32)` in the allowlist. This is exactly correct for SET_XSAVE:
  per the SDM XRSTOR pseudocode, MXCSR is restored from the save area whenever
  `RFBM[1] OR RFBM[2]` is set — **independent of XSTATE_BV[1]**. So clearing bit 1 while keeping
  the MXCSR bytes does NOT reset MXCSR to 0x1F80 on restore; the guest's real MXCSR survives. The
  prompt's worry ("does normalizing SSE-init-with-nonDefault-MXCSR change restored MXCSR?") is
  **answered: no** — *because* MXCSR is never zeroed here. This is correct but currently
  *undocumented as a deliberate restore-safety invariant*; if a future edit ever moved MXCSR out
  of the allowlist (e.g. "tidied" it under the SSE bit) it would silently corrupt restored MXCSR.

**Recommended fix:** add a `# Restore safety (55f)` paragraph to the doc comment, e.g.:

```rust
/// # Restore safety (55f, KVM_SET_XSAVE)
/// MXCSR/MXCSR_MASK [24,32) is ALWAYS kept, never zeroed, never gated on a
/// bit. XRSTOR loads MXCSR from the area whenever RFBM[1]|RFBM[2] is set,
/// independent of XSTATE_BV[1]; clearing bit 1 (SSE-init) therefore does NOT
/// reset restored MXCSR to 0x1F80. Do not move MXCSR under the SSE bit.
/// Clearing a bit whose area is the EXACT init pattern is restore-safe ONLY
/// for components whose architectural init state is all-zero (x87 via
/// is_x87_init, SSE via all-zero XMMs). See I1 before reusing the generic
/// extended-bit normalization on the restore path.
```

This is a documentation-only change but it is the difference between a future 55f author knowing
the invariant and re-introducing the corruption the prompt feared.
