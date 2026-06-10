# Decision: guest-TSC alignment uses the KVM_VCPU_TSC_OFFSET attribute

**Bead:** determinism-hypervisor-3np · **Status:** decided 2026-06-10 ·
**Owner mechanism:** `crates/dh-vmm/src/tsc.rs`

## Context

ARCH §4.4's restore rule writes guest TSC ← vns. Two mechanisms exist
(ARCH §4 defense-4 caveat):

1. **Per-entry `KVM_SET_MSRS{IA32_TSC}` value writes** — engage KVM's
   TSC-sync heuristics (a write landing inside the kernel's matching
   window can be quantized onto an existing sync generation: a silent
   value perturbation, i.e. a determinism hazard), and would have to be
   re-issued per entry to stay aligned.
2. **The `KVM_VCPU_TSC_CTRL`/`KVM_VCPU_TSC_OFFSET` vCPU device
   attribute** — sets the exact hardware TSC offset once
   (`guest_tsc = host_tsc + offset`); no heuristics; survives entries.

## Measured (lab box, infra-control, kernel 6.8, 2026-06-10)

| mechanism | ns/call, release (N=10,000) |
|---|---|
| `KVM_SET_DEVICE_ATTR(TSC_OFFSET)` | **932** |
| `KVM_SET_MSRS{IA32_TSC}` | 1,107 |

(Release-build numbers; review verified the gap is the ioctl itself,
not the `Msrs` allocation — hoisting it does not narrow it.)

At the §10 envelope of ~3k exits/guest-second, per-entry MSR writes
would cost ≈ **3.3 ms per guest-second** — and carry
the sync-heuristic hazard regardless. The offset attribute is set
**once per restore**, round-trips **bit-exactly** (verified in
`tsc.rs` tests: set −123,456,789 → read −123,456,789), and costs
nothing on the run path.

## Decision

**The TSC offset attribute is the M4 restore mechanism.** Restore
computes `offset = vns − host_tsc_at_resume` and issues one
`KVM_SET_DEVICE_ATTR`. Per-entry MSR writes are retained in
`tsc.rs::set_tsc_value_msr` ONLY as a benchmarked reference and must
not be wired into restore.

**Units:** ARCH §4.1/§8.3 define the virtual TSC's unit AS vns (one
tick = one virtual nanosecond), so `offset = vns − host_tsc_at_resume`
needs no frequency conversion. After resume the guest TSC advances at
the HOST rate while vns advances per the clock rational — the drift is
intended (§4 defense 4: guests must take time from pv-clock; the TSC is
merely monotonic).

Implementation notes for the M4 codec: `KVM_GET_DEVICE_ATTR` is `_IOW`
(not `_IOWR`) in the kernel uapi — the kernel writes through
`attr.addr`; kvm-ioctls 0.24 cfg-gates its device-attr wrappers to
aarch64, so dh-vmm issues the ioctls raw (see `tsc.rs`).
