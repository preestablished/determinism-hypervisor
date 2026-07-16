# Critical & Important

## Critical

**None.** The canonicalization transform is correct, the bounds checks are loud,
the offsets/sizes are validated live against this host's CPUID 0xD, and no pinned
hash is broken. Verified on real hardware.

---

## Important

### I-1 — `get_xsave()` (fixed 4096 B) truncates on AVX-512 + AMX hosts; file for 55f

- **File:** `crates/dh-vmm/src/hash.rs:267` (and the live wiring everywhere
  `vcpu.get_xsave()` feeds `canonicalize`), `crates/dh-vmm/src/xsave.rs` (the
  `KVM_GET_XSAVE` assumption baked into the module doc and `live_tests`).
- **Severity:** Important (latent, not live on the lab box).

**What I found, live.** `kvm-bindings 0.14` `kvm_xsave.region` is `[u32; 1024]`
(4096 bytes). `kvm-ioctls 0.24` exposes BOTH `get_xsave()` (old fixed ioctl) and
`get_xsave2()` (`KVM_GET_XSAVE2`, for areas > 4096). The code uses the fixed one.

On THIS host that is fine: host XCR0 = 0x1f → area 1088 B, and
`KVM_CHECK_EXTENSION(KVM_CAP_XSAVE2)` returns 4096 (it clamps to the legacy
minimum). On a host that *enables* AVX-512 (opmask/ZMM_Hi256/Hi16_ZMM ≈ +2 KiB)
plus AMX (XTILECFG + XTILEDATA, the latter 8 KiB), the host XSAVE area exceeds
4096 and `KVM_CAP_XSAVE2` reports the larger size. `KVM_GET_XSAVE` then either
truncates (components past 4096 silently dropped) or, depending on kernel
version, returns the legacy subset — either way the hash preimage would be
**incomplete**, not wrong-but-stable.

**Why this is not Critical here.** Two mitigations make it fail closed rather than
silently corrupt:

1. `host_component_layout()` reads the *real* host CPUID 0xD (not the masked
   guest table). On a big host it enumerates a component with `offset ≥ 4096`.
   `canonicalize` checks `e <= area.len()` (4096) and returns
   `ComponentOutOfBounds { bit }` → surfaced as `KvmError::Open(...)` in
   `canonical_vcpu_blob`. So the hypervisor refuses to produce a silently-truncated
   hash on such a host.
2. The lab box is the only target today; guest CPUID 0xD is fully zeroed so the
   guest can never grow XCR0.

**Recommended fix (in bead 55f, not this bead).** Switch the capture to
`get_xsave2()` sized from `KVM_CAP_XSAVE2`, OR add an explicit guard at slot/probe
time that hard-fails if `KVM_CAP_XSAVE2 > 4096` while still on the fixed ioctl —
so the truncation risk is documented and asserted, not merely implied by an
out-of-bounds error deep in the hash path. Sketch of the guard:

```rust
// At KvmSystem::open() or the preflight smoke:
let xsave2 = kvm.check_extension_int(Cap::Xsave2); // or check_extension_raw(KVM_CAP_XSAVE2)
if xsave2 > 4096 {
    return Err(KvmError::Open(format!(
        "host XSAVE area {xsave2} B exceeds the fixed KVM_GET_XSAVE region; \
         capture must use KVM_GET_XSAVE2 (bead 55f)"
    )));
}
```

This belongs in 55f because that bead already owns the DHSNAP vCPU-section reuse
of this same transform; the ioctl choice is the natural companion decision.
**Action: file a bead-55f note capturing the live measurements above.**
