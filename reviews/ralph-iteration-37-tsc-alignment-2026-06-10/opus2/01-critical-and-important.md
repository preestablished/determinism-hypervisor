# Critical & Important findings

## CRITICAL — `get_tsc_offset` reads through a shared ref to an un-written local: UB that produces a wrong value in release

**File:** `crates/dh-vmm/src/tsc.rs:77-90` (and the `attr_for` helper at `:39-46`).

```rust
pub fn get_tsc_offset(vcpu: &VcpuFd) -> Result<i64, KvmError> {
    let raw = 0u64;                       // immutable, never written by Rust
    let attr = attr_for(Some(&raw));      // addr = (&raw as *const u64) as u64
    let rc = unsafe { ioctl_with_ref(&vcpu.as_raw_fd(), KVM_GET_DEVICE_ATTR(), &attr) };
    ...
    Ok(raw as i64)                        // compiler assumes raw is still 0
}
```

### Why it is unsound

`KVM_GET_DEVICE_ATTR` is `_IOW` (correctly noted in the code): the kernel does **not** write the
`kvm_device_attr` struct — it writes 8 bytes to the address in `attr.addr`. Here that address is derived
from a **shared** reference `&raw` to a plain (non-`UnsafeCell`) stack local that Rust never mutates. In
the Rust/LLVM memory model a `*const T` carries no permission to mutate the pointee, and a local borrowed
only `&` and never written is assumed unchanged. The optimizer is therefore free to fold `Ok(raw as i64)`
to `Ok(0)`. This is the textbook strict-provenance / "interior mutation through `*const`" violation. In a
determinism hypervisor, latent UB that the optimizer is licensed to exploit is **Critical** even if a
given build "works".

### It is not theoretical — reproduced live on this box

```
$ cargo test -p dh-vmm --release --lib tsc:: -- --test-threads=1
thread '...tsc_mechanisms_work_and_benchmark_live' panicked at crates/dh-vmm/src/tsc.rs:138:9:
  assertion `left == right` failed
  left: 0
 right: -123456789
```

`debug` passes; `release` fails, deterministically (reproduced 4×). The committed test
`assert_eq!(get_tsc_offset(...), -123_456_789)` is a **release-mode bug detector that CI never trips**,
because `.github/workflows/ci.yaml` runs `cargo test --workspace` with no `--release` on both the
host and the kvm-intel lanes, and there is no Miri lane.

### Compare to the blessed pattern

kvm-ioctls 0.24 (`src/ioctls/vcpu.rs`) routes every ioctl where the **kernel writes the struct** through
`ioctl_with_mut_ref(self, …, &mut x)` (`get_regs`, `get_sregs`, `get_xsave2`, …). For device attrs it
exposes only `set_device_attr`/`has_device_attr` (kernel reads), via `ioctl_with_ref(&attr)` — which is
exactly why the project's `set_tsc_offset`/`has_tsc_offset_attr` are fine: there the kernel only **reads**
`raw`, so the optimizer keeping `raw == offset` is correct. The asymmetry is the whole bug: GET writes,
and the current code hands the kernel a write target that Rust believes is immutable.

### Minimal sound fix (verified live: release round-trip passes)

Give `get_tsc_offset` its own builder using a **mutable** local and a pointer with write provenance:

```rust
pub fn get_tsc_offset(vcpu: &VcpuFd) -> Result<i64, KvmError> {
    let mut raw = 0u64;
    let attr = kvm_device_attr {
        flags: 0,
        group: KVM_VCPU_TSC_CTRL,
        attr: u64::from(KVM_VCPU_TSC_OFFSET),
        addr: std::ptr::addr_of_mut!(raw) as u64,   // *mut provenance, escaped
    };
    let rc = unsafe { ioctl_with_ref(&vcpu.as_raw_fd(), KVM_GET_DEVICE_ATTR(), &attr) };
    if rc != 0 { /* err */ }
    Ok(raw as i64)
}
```

`addr_of_mut!` on a `mut` local materializes a write-capable pointer whose address escapes opaquely into
`attr.addr`; the optimizer can no longer prove `raw` is unchanged across the FFI call. With this change,
`cargo test --release` rounds `-123456789` back exactly. (Belt-and-suspenders alternatives:
`std::hint::black_box(&raw)` after the call, or `core::ptr::read_volatile(&raw)` to load the result —
but `addr_of_mut!` is the minimal, idiomatic, allocation-free fix and matches how the rest of the
ecosystem types kernel-written buffers.) The `attr_for(Some(&raw))` helper should keep serving the
read-only SET/HAS paths; only GET needs the mutable-local variant.

### Severity rationale

GET is currently only a "verification + diagnostics" helper, but the decision doc names it the M4
verification step ("verified in `tsc.rs` tests: set → read"). A verification primitive that silently
returns `0` in the production (release) profile gives **false confidence that restore aligned the TSC**.
That is precisely the failure class this hypervisor exists to prevent. Fix before M4 consumes it.

---

## IMPORTANT — decision doc lacks the post-resume TSC-vs-vns drift caveat (and a §8.3 sequencing note)

**File:** `docs/decisions/tsc-alignment.md:36-40`, restore formula.

### The formula itself is correct — do not "fix" it

`offset = vns − host_tsc_at_resume` is **dimensionally sound under the architecture's own definition** and
needs no `× host_freq` factor. ARCH §4.1 defines `vns = icount·clock_num/clock_den` (default 1:1, "one
virtual nanosecond per retired instruction — deterministic 1 GHz") and §8.3 restore writes `IA32_TSC ←
vns` directly. The spec thereby *defines the virtual TSC's unit to be the virtual nanosecond* — a 1 ns
tick. Setting `offset = vns − host_tsc` makes `guest_tsc = host_tsc + offset = vns` at the resume instant,
in matching units. There is no unit bug. (I verified the architecture's intent by reading §4.1 and §8.3;
the "IA32_TSC ← vns" rule is the load-bearing premise.)

### What is missing: the drift caveat

After resume, the hardware TSC counts at the **host** frequency (≈3 GHz on the Coffee Lake lab box per
the §2.1 empirics note), while `vns` advances at the rational rate (≈1 GHz default). So `guest_tsc`
immediately drifts *above* `vns`, growing at `(host_freq − 1 GHz)` per second. A guest that did
`rdtsc; pv-clock read; rdtsc` would compute its TSC frequency as the host's, not 1 GHz — and the CPUID
0x15/0x16 zeroing (cpuid.rs:75) deliberately denies it any other frequency source.

This divergence is **architecturally fine and intended** — ARCH §4 defense-4 states the alignment makes a
stray RDTSC "approximately virtual and drifts only between exits", and defense-1 forbids RDTSC as a guest
time source (guests use dh-pvclock; `tsc=unstable`). But the decision doc presents the formula as if it
produces an enduring `guest_tsc == vns`, which it does not. An M4 implementer reading only this doc could
(a) think the offset must be re-applied per entry to hold the equality (re-introducing the very per-entry
cost/hazard the decision rejects), or (b) "discover" the drift and add a bogus frequency conversion.

**Recommended doc amendment** (one paragraph): "The offset pins `guest_tsc == vns` only at the resume
instant. Thereafter the hardware TSC advances at the host frequency while vns advances at the clock
rational, so guest TSC drifts from vns at `(host_freq − clock_freq)`. This is intended (ARCH §4 defense-4:
TSC is 'approximately virtual'); the guest's only sanctioned time source is pv-clock, and CPUID
0x15/0x16 are zeroed so the guest cannot calibrate against the host frequency. The offset is therefore set
**once per restore** and never re-applied per entry."

### Secondary: §8.3 vs the decision contradict on the literal mechanism

ARCH §8.3 step 3 says restore should "set MSRs last, IA32_TSC ← vns" — i.e. a `KVM_SET_MSRS{IA32_TSC}`
value write, the *rejected* mechanism. The decision switches restore to the offset attribute. That is the
right call, but §8.3's wording now disagrees with the decision. The doc should (a) note that §8.3's
"IA32_TSC ← vns" is realized via the offset attribute, not a value write, and (b) flag the **ordering
hazard for M4**: if restore both runs `KVM_SET_MSRS{IA32_TSC}` (per §8.3's literal list) *and* sets the
offset, the two fight and the MSR write re-engages the sync heuristic. M4 must apply the offset attribute
*instead of* an IA32_TSC entry in the SET_MSRS list, and `IA32_TSC` should be dropped from the restore
MSR write set (it stays on the capture list only for diagnostics, per §8.1 line 645). Recommend either
amending §8.3 or adding an explicit "supersedes §8.3 mechanism" line to the decision.
