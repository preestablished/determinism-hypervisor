# Critical & Important findings

## Critical

**None.** No correctness, safety, or determinism bug exists *within*
`vcpu_state.rs`. Every live round trip on this kernel is a fixed point,
padding is empirically zero, and cross-VM encodings are byte-identical.

---

## Important

### I1 — `encode_section` is a SECOND, divergent VCPU-section encoder; `dh-snapshot` already names `canonical_vcpu_blob` as *the* one

- **File:** `crates/dh-vmm/src/vcpu_state.rs:261-278` (`encode_section`) vs
  `crates/dh-vmm/src/hash.rs:176` (`canonical_vcpu_blob`); design intent at
  `crates/dh-snapshot/src/dhsnap.rs:15` and the codec test
  `crates/dh-snapshot/tests/dhsnap_codec.rs:12`.
- **Severity:** Important (latent integration trap; not a bug on this branch
  in isolation, but it will bite the qmp/9e4 wiring).
- **What I found (empirically + by reading both encoders):** there are now
  **two non-identical definitions of "the DHSNAP VCPU section":**

  | aspect | `encode_section` (this branch) | `canonical_vcpu_blob` (hash.rs, committed) |
  |---|---|---|
  | encoding style | raw `repr(C)` struct byte-copy (`struct_bytes`), **padding included** | field-by-field manual LE emit, **padding excluded** (e.g. emits only `interrupt_bitmap[0..4]`, segment fields one at a time) |
  | MSR list | `RESTORE_MSR_LIST` = **14** entries, **no IA32_TSC** | `MSR_CAPTURE_LIST` + `TSC_SLOT_AT=13` = **15** entries, **normalized IA32_TSC(vns) inserted** between TSC_AUX and SPEC_CTRL |
  | decode counterpart | yes (`decode_section`) | none (hash preimage is encode-only) |

  `dhsnap.rs:15` states the VCPU section is "dh-vmm's `canonical_vcpu_blob`",
  and `dhsnap_codec.rs:12` annotates the VCPU push as
  `// canonical_vcpu_blob (dh-vmm owns)`. So the snapshot store and the hash
  chain are already committed to `canonical_vcpu_blob`'s bytes. If qmp/9e4
  serialize the VCPU section with `encode_section` instead, the **stored
  section bytes ≠ hash preimage**, and the snapshot-ref hash will not be a
  function of the bytes actually on disk. The prompt's premise ("the section
  bytes feed byte-equality and eventually the snapshot ref hash") is, on this
  branch, **only half true**: `encode_section` feeds `VcpuState`'s byte-equality
  and the cross-fork compare, but the *hash* is fed by the **other** encoder.

- **Why both legitimately exist:** `canonical_vcpu_blob` is encode-only (you
  never decode a hash); restore genuinely needs an *invertible* codec, which is
  exactly what `encode_section`/`decode_section` provide. So deleting one is not
  the fix. The fix is to make the relationship explicit and the bytes reconciled.
- **Concrete fix the bead must land (flag for qmp/9e4):**
  1. Decide which byte stream the DHSNAP `VCPU` section actually carries and
     state it in `dhsnap.rs`. If it is `encode_section`'s output (so it can be
     decoded on restore), then `hash.rs::canonical_vcpu_blob` must hash **that
     same byte stream** (or be retired in favor of hashing the section bytes),
     and the IA32_TSC-slot insertion must be reconciled.
  2. If the two must stay separate, add a doc note + a single
     unit test in `vcpu_state.rs` (or `hash.rs`) that pins the **intentional**
     differences (MSR count 14 vs 15, padding-included vs -excluded) so a
     future reader cannot assume `hash == H(section)`.
  3. The MSR divergence specifically: `encode_section` writes `count=14` and
     emits TSC_AUX immediately followed by SPEC_CTRL; the hash inserts a 15th
     (IA32_TSC=vns) record at slot 13. A restore decoder fed a 15-entry section
     would reject it (`decode_section` hard-checks `count == 14`), and vice
     versa. These two list lengths must not both be presented as "the §8.1 list".

### I2 — `restore` computes the TSC offset from `rdtsc()`, but ARCH §4 defense-4 says "align the guest TSC to vns on **every VM entry**"; the unbounded rdtsc→entry skew is unhandled and the per-entry alignment lives nowhere

- **File:** `crates/dh-vmm/src/vcpu_state.rs:187-189`; spec at
  `ARCHITECTURE.md:367-375` (defense 4) vs `docs/decisions/tsc-alignment.md:40-50`.
- **Severity:** Important — not a `vcpu_state.rs` bug (the offset write is
  exactly what the *decision doc* mandates), but a **spec reconciliation gap**
  qmp/9e4 inherit.
- **What I found:**
  - `restore` does `offset = vns.wrapping_sub(rdtsc())` then one
    `KVM_SET_DEVICE_ATTR(TSC_OFFSET)`. That matches
    `tsc-alignment.md`'s `offset = vns − host_tsc_at_resume` exactly, set
    **once per restore**, and the decision doc explicitly accepts that
    post-resume the guest TSC drifts (advances at host rate) — "the drift is
    intended (§4 defense 4)".
  - **But** ARCH §4 defense-4 (line 367) still reads: "On **every VM entry**
    after an exit, the VMM aligns the guest TSC to `vns` at the entry boundary,
    so even a stray kernel RDTSC reads a value that is *approximately* virtual
    and drifts only between exits." The decision doc *narrowed the mechanism*
    (offset attr, once) but did **not** delete the per-entry-alignment
    requirement from ARCH — and there is **no per-entry realignment anywhere in
    the run loop** (`grep set_tsc_offset` finds only `tsc.rs` and this
    `restore`; `run.rs`/`runctl.rs` have none). So today a snapshot resumes
    aligned, then the guest TSC drifts unbounded until the next *restore*, never
    re-aligned per entry.
  - Separately, the `rdtsc()` here is read in userspace; an arbitrary
    preemption can occur between this read and the first `KVM_RUN`, so even the
    initial alignment is off by the scheduling delay. The offset-attr mechanism
    can't tighten this without reading the **current KVM-visible guest TSC**
    (or host TSC at entry) rather than a userspace `rdtsc()` snapshot.
- **Why it round-trips green anyway:** the live test's "fixed point" compares
  `VcpuState` (regs/sregs/.../msrs) which does **not** include IA32_TSC
  (deliberately excluded from `RESTORE_MSR_LIST`), so TSC skew is invisible to
  the equality. The test asserts the offset is merely `!= 0`, not its value.
  The round trip passing tells you nothing about TSC alignment precision.
- **Concrete flag for qmp/9e4 (and a doc owner):**
  1. Reconcile ARCH §4 defense-4 with `tsc-alignment.md`: either delete
     "every VM entry" from ARCH (declaring once-per-restore the contract, with
     intended drift) **or** wire a per-entry `set_tsc_offset` into the run loop.
     Pick one; right now the two docs disagree and the code implements neither
     the "every entry" wording.
  2. If precision at resume matters, compute the offset from the **KVM-visible
     guest TSC** (read it back via the attr / `KVM_GET_MSR(IA32_TSC)` path) and
     adjust the delta, instead of trusting a userspace `rdtsc()` that the
     scheduler can separate from the entry by an unbounded interval.
  3. Because this is a *decided* mechanism ("do not reopen" per the module
     doc), the actionable part for this bead is **documentation**: have
     `vcpu_state.rs::restore` reference that per-entry alignment is the run
     loop's job (or explicitly out of scope), so the next reader does not
     assume `restore` satisfies ARCH §4 defense-4.
