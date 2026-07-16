# Suggestions (non-blocking)

## S1 — Serialize `exception_has_payload` / `exception_payload` (or assert them zero)

**File:** `crates/dh-vmm/src/hash.rs:248–267`

`kvm_vcpu_events` in kvm-bindings 0.14 carries two trailing fields beyond what the blob
serializes (x86_64 bindings.rs:1736–1737):

```rust
pub exception_has_payload: __u8,
pub exception_payload:     __u64,
```

These are populated only when `events.flags & KVM_VCPUEVENT_VALID_PAYLOAD` is set (newer
kernels, used for some `#PF`/`#DB`-class deliveries via `KVM_CAP_EXCEPTION_PAYLOAD`). The blob
serializes `events.flags` (line 262) but **not** these two fields, so two states that differ
only in `exception_payload` hash equal.

For the Phase-1 boot/quiescent-boundary guest this is almost certainly never non-zero, so the
omission is **probably harmless today** — but it is a silent state-coverage gap that the
module docs don't call out. Two cheap options:

- include both fields in the blob unconditionally (8 + 1 bytes; matches the "serialize every
  logical field" principle used for `triple_fault.pending` and `smi.*`), **or**
- if you intend to scope them out for Phase 1, add an explicit `debug_assert!`/runtime check
  that `flags & KVM_VCPUEVENT_VALID_PAYLOAD == 0` and a doc line, so an unexpected payload
  becomes a loud failure rather than a silent hash collision.

I'd take option 1 — it costs nothing and removes the asterisk.

## S2 — Bind the FPU/SREGS field lists to a compile-time size assertion

**File:** `crates/dh-vmm/src/hash.rs:143–156`, `233–246`

The serializers enumerate struct fields by hand and correctly skip padding (`kvm_segment`
padding; `kvm_fpu.pad1`/`pad2`). That correctness is invisible to the compiler — a future
kvm-bindings bump that adds a field will compile silently and the blob will quietly omit it.
Consider a `const _: () = { … size_of::<kvm_fpu>() … }`-style guard, or a unit test that
asserts the expected `size_of` of each source struct, so a binding change that grows a struct
forces a human to revisit the serializer. (kvm-bindings already ships exactly these size
asserts internally; mirroring one here pins your assumption.)

## S3 — Factor the link-hashing core shared by `push_link` and `push_final_link`

**File:** `crates/dh-vmm/src/hash.rs:93–110`, `124–138`

The two methods duplicate the header (`H_i || blob || sections`) and trailer
(`le64(icount) || le64(vns)`) hashing inline. The duplication is small and the prompt's
worry about borrowing the stack page buffer per iteration is valid — but you could still share
the head/tail via a tiny helper that takes the already-opened `Hasher`, leaving only the page
loop different. Low priority; the current form is readable and the ascending invariant is
correctly handled differently in each (asserted in `push_link`, true by construction in the
`0..n` loop). Acceptable as-is; noting only because the two preimage definitions must stay in
lockstep forever and a shared helper makes drift impossible.

## S4 — Tighten the `device_sections` length cast

**File:** `crates/dh-vmm/src/hash.rs:315`

`section.len() as u32` silently truncates a device section larger than 4 GiB. No DetDevice
will ever produce that, so this is theoretical, but a debug-time `debug_assert!(section.len()
<= u32::MAX as usize)` documents the invariant and prevents a catastrophic preimage corruption
if some future device blob grows unexpectedly. (Same pattern would apply if you adopt the
link-level length prefixes from Important #2 — use `u64` there for headroom.)

## S5 — Add a host-side golden-vector test pinning the preimage layout

**File:** `crates/dh-vmm/src/hash.rs` tests module

The existing tests prove determinism, position-sensitivity, perturbation-sensitivity, and the
ascending panic — all excellent. What they do **not** pin is the **exact byte layout / field
order** of the preimage. A single golden test that feeds fixed inputs and asserts a hardcoded
32-byte hash would lock the layout, so any of the reorderings discussed in 01 (MSR order,
field additions, framing) trips a red test instead of silently shifting the hash. This is the
cheapest insurance for a value defined as normative in §8.5. Pair it with the §8.1-order fix
(Important #1) so the golden vector encodes the *corrected* order.
