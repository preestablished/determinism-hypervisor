# Positive notes — good patterns worth keeping

### P1 — The padding question is *empirically* clean, and for the right reason
My live audit confirmed every reserved/padding range zero across two fresh
VMs, with **byte-identical** cross-VM encodings. Crucially this is not luck:
KVM's `GET_*` ioctls zero the reserved fields, so the raw byte-copy carries no
instance-specific garbage — exactly the property the iteration-69 XSAVE fix had
to *manufacture* via canonicalization, here provided by the kernel for free.
The code correctly recognizes XSAVE is the one structure that *does* need
canonicalization (`capture` runs `crate::xsave::canonicalize`) and leaves the
others as plain copies. That's the right discrimination.

### P2 — Fail-closed on XSAVE2 instead of silent truncation
`capture` (vcpu_state.rs:91-100) probes `KVM_CAP_XSAVE2` and **hard-errors** if
the host's XSAVE area exceeds 4096, rather than truncating into a fixed
`kvm_xsave`. On an AMX-class host this would otherwise silently drop component
state — a determinism landmine. Failing loud with a precise message (and a
pointer to the XSAVE2 follow-up) is exactly the §2.1 "fail closed" philosophy.

### P3 — Restore ordering is correct and the *why* is documented inline
SREGS→REGS→VCPU_EVENTS, FPU-before-XSAVE (XSAVE authoritative for the
x87/SSE overlap), and especially **XCRS-then-XSAVE** with the explicit note
that the reverse re-inits enabled components (XSETBV after XRSTOR). This is a
genuinely easy ordering to get wrong; the comments capture the hardware reason,
not just the order. Live round trip confirms it holds.

### P4 — EFER double-set is benign and now verified
EFER is written twice during restore (via SET_SREGS, then via SET_MSRS). My
live experiment confirmed restore succeeds and re-capture is byte-identical.
This is benign because both writes carry the **same captured value**: KVM
validates EFER against guest mode, but setting it to a value already consistent
with the SREGS-established mode (then re-asserting the identical value) is a
no-op on the second write. There is no ordering hazard as long as the two
sources agree — and they do, because both come from the same `capture`.

### P5 — Decode is strict and total
`decode_section` rejects wrong version, truncation, trailing bytes, wrong XSAVE
length, a nonzero MSR `_pad`, and any MSR index diverging from the
code-versioned `RESTORE_MSR_LIST`. The malformed-section test exercises each.
For a codec that gates restore, "reject anything unexpected" is the correct
posture, and the bounds checks in `read_struct` (checked_add + length filter)
are clean — no possibility of an out-of-bounds copy from attacker-controlled
length fields.

### P6 — `VcpuState::PartialEq` defined as section-byte equality
Defining equality as `encode_section(self) == encode_section(other)` makes the
"determinism-relevant equality" the canonical one and prevents a future field
addition from being silently excluded from comparison. (The one caveat — that
this is a *different* byte stream from the hash preimage — is I1, but the
intent here is sound.)

### P7 — SAFETY comments are specific, not boilerplate
Every `unsafe` block names the invariant it relies on (repr(C) POD, any bit
pattern valid, bounds-checked source range, slice lifetime tied to the borrow).
The `read_struct`/`struct_bytes` pair is the kind of `unsafe` that's actually
auditable.
