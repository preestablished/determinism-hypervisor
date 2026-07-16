# Critical and Important Findings

No **Critical** findings. The chain is internally consistent and correct for the M3
run-twice-compare goal it targets. The three Important findings below are all about keeping
the §8.5/§8.1 preimage agreeing with the future M4 DHSNAP codec — cheap to fix now, costly
(a hash-version bump and re-baselining) after release.

---

## Important #1 — IA32_TSC preimage position deviates from §8.1 document order

**File:** `crates/dh-vmm/src/hash.rs:43–58`, `277–301`

§8.1 (ARCHITECTURE.md:643–646) lists the MSR capture set as:

> EFER, STAR, LSTAR, CSTAR, SFMASK, KERNEL_GS_BASE, FS_BASE, GS_BASE, SYSENTER_{CS,ESP,EIP},
> PAT, TSC_AUX, **IA32_TSC** (normalized…), **SPEC_CTRL**.

So the documented order places **IA32_TSC before SPEC_CTRL**. The implementation:

- omits `IA32_TSC` (0x10) from `MSR_CAPTURE_LIST` (correct — it is never read via
  `KVM_GET_MSRS`; its value is the normalized `vns`), and
- appends the normalized TSC slot (`0x10 || le64(vns)`) **after** the whole list, i.e.
  **after SPEC_CTRL** (hash.rs:294–301).

The result is preimage order `… PAT, TSC_AUX, SPEC_CTRL, IA32_TSC` — TSC and SPEC_CTRL are
**transposed** relative to §8.1.

**Why it matters:** Today this is harmless because the only producer and the only consumer
of this preimage are *this code* (M3 run-twice-compare). But the M4 DHSNAP codec is specified
to serialize the vCPU section in §8.1 order (§8.5 line 720: "DHSNAP vCPU section bytes,
canonicalized §8.1"). When M4 emits `… TSC_AUX, IA32_TSC, SPEC_CTRL`, the two preimages
disagree and the state hashes computed by the live path and by the codec path will differ
for identical logical state. That is exactly the cross-service comparison §8.5 line 729
promises will work.

**Recommendation (fix now, pre-release):** Place the normalized IA32_TSC slot **between
TSC_AUX and SPEC_CTRL** in the preimage, matching §8.1. Cheapest implementation: keep
SPEC_CTRL out of `MSR_CAPTURE_LIST`, end the read list at TSC_AUX, emit the normalized
`0x10 || vns` slot, then read+emit SPEC_CTRL separately — or simply reorder the post-loop
emission. Add a code comment binding the order to §8.1 and a host-side golden-vector test so
a future reorder is caught. (See Important #3 for why reading SPEC_CTRL separately also helps
robustness.)

---

## Important #2 — `vcpu_blob` and `device_sections` are concatenated without length prefixes

**File:** `crates/dh-vmm/src/hash.rs:93–96`, `124–126`

In both `push_link` and `push_final_link` the link preimage is:

```
H_i || vcpu_blob || device_sections || (pages…) || le64(icount) || le64(vns)
```

`vcpu_blob` and `device_sections` are variable-length byte strings concatenated with **no
length delimiter between them**. The boundary is recoverable today only because both fields
have a fixed/known internal structure produced by this code. But as a hash preimage this is
**ambiguous**: moving the trailing K bytes of `vcpu_blob` to the front of `device_sections`
(or vice versa) yields the **same preimage and the same hash**. The §8.1 list-order and
field-set are *implicit* framing; nothing in the bytes themselves marks where the blob ends
and the sections begin.

Note the codebase already knows this pattern is dangerous: `config.rs` has a test
`length_prefixes_prevent_ambiguity`, and `device_sections()` itself frames each device with
`(id, version, len, bytes)`. The inter-field boundary in the *link* is the one place the
discipline lapses.

**Why it matters now:** The M4 codec will independently decide how to lay out the vCPU
section vs the device sections. If its framing differs by even one byte at that boundary, or
if a future change makes `vcpu_blob` variable-length in a way that lets bytes migrate across
the boundary, the hash silently diverges or (worse) two genuinely different states collide.
The §8.5 normative text does not mandate length prefixes, so this is also a chance to tighten
the spec.

**Recommendation (fix now, pre-release):** Prefix each variable-length field in the link with
`le64(len)` — at minimum `le64(vcpu_blob.len()) || vcpu_blob || le64(device_sections.len()) ||
device_sections`. This is a preimage change, so do it before any `dh-statehash-v1` artifact is
treated as a baseline, and update §8.5 to make the framing normative. Cost is 16 bytes per
link; the collision/ambiguity elimination is worth it for a value "exchanged with other
services."

---

## Important #3 — strict `n != len` on `KVM_GET_MSRS` makes `push_final_link` fail on hosts lacking a listed MSR

**File:** `crates/dh-vmm/src/hash.rs:287–293`

```rust
let n = vcpu.get_msrs(&mut msrs).map_err(kvm_err("KVM_GET_MSRS"))?;
if n != MSR_CAPTURE_LIST.len() {
    return Err(KvmError::Open(format!("KVM_GET_MSRS returned {n}/{} entries", …)));
}
```

`KVM_GET_MSRS` returns the **number of entries successfully read** and **stops at the first
index it cannot read**, returning that position (the kvm-ioctls 0.24 contract is exactly
"returns the number of MSR entries read", vcpu.rs:721). On a host where one of the listed
MSRs is unsupported — most plausibly **SPEC_CTRL (0x48)** on older CPUs without IBRS, but the
same applies to any list member — KVM returns `n < len` and this code turns it into a hard
error. `push_final_link` then fails on that host, even though the *guest* never touched the
missing MSR.

This is the classic robustness-vs-loud-failure tradeoff. I lean toward calling it
**Important** rather than a Suggestion because:

- the failure is **host-dependent and silent until it happens** (CI on a modern host passes;
  an older deployment host fails at the worst possible moment — first capture);
- the hash is supposed to be **portable across the worker fleet**, and a list member's
  presence is a property of the host CPU, not the guest;
- the §8.1 completeness argument ("filter would have faulted unsupported writes") guarantees
  the guest never *wrote* a non-list MSR, but it does **not** guarantee every list member is
  *readable* on every host.

**Recommendation:** Decide the policy explicitly and encode it:

- **Preferred (robust + deterministic):** make the capture-MSR set a property of the
  **machine config / host caps gate**, validated once at slot creation (fail fast at preflight
  if a required MSR is absent), so by the time `push_final_link` runs the set is known-readable.
  This keeps the loud failure but moves it off the hot path and out of the hash function.
- **Acceptable alternative:** if a host legitimately lacks SPEC_CTRL, define a deterministic
  preimage for "absent" (e.g. emit the index with a sentinel/zero and record absence in the
  machine_config_hash so two hosts with different MSR availability never compare equal by
  accident). This must be machine-config-bound, never silently host-varying.

Either way, add a comment at the check site explaining the partial-return semantics and why
the chosen policy is safe. As written, the current strict check is defensible **only** if a
preflight gate already guarantees all listed MSRs are readable — and I did not find such a
gate covering SPEC_CTRL specifically (msr.rs gates the *guest filter*, not host readability).
