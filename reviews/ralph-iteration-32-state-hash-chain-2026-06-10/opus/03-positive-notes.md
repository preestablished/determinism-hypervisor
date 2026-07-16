# Positive Notes

## P1 — Honest, specific Phase-1 scoping with in-code rationale
The module header (hash.rs:1–26) states exactly what is and isn't covered: full-RAM walk
now / dirty-delta at M4, non-XSAVE subset now / XSAVE canonicalization at M4, normalized TSC
now / raw-TSC alignment at M2. Each deferral names the milestone that closes it and asserts
"M4 extends, never replaces — same harvest order." This is the right discipline for a hash
that other milestones must reproduce, and it makes the review tractable.

## P2 — Field-by-field LE serialization, never raw struct memory
`canonical_vcpu_blob` and `seg()` serialize each logical field explicitly and **skip padding**
(`kvm_segment` padding; `kvm_fpu.pad1`/`pad2`). This is the correct way to hash KVM structs —
raw `as_bytes()` would fold non-deterministic padding into the state hash. Verified against
kvm-bindings 0.14: every non-padding `kvm_fpu` field is present and the two pad fields are the
only ones omitted.

## P3 — SREGS deviation is correctly forced, not careless
The bead asked for SREGS2, but kvm-ioctls 0.24 (the pinned dependency) exposes **no
`get_sregs2` method at all** — confirmed by inspecting `src/ioctls/vcpu.rs` (only `get_sregs`,
line 378) and grepping the whole crate for `sregs2` (zero hits). The code uses the only
available ioctl and documents *why* (lines 202–204: SREGS2's pdptr extension matters only for
PAE-without-LMA guests, not this machine; M4's codec owns the upgrade). That is exactly the
right way to handle a spec/dependency gap.

## P4 — Strict ascending-page invariant as a loud assertion
`push_link` asserts both `bytes.len() == PAGE_SIZE` and strict ascending index, with clear
panic messages, and the docstring frames violations as a caller bug ("not a guest-influenced
path"). The `out_of_order_pages_panic` test pins it. `push_final_link`'s `0..n` loop is
ascending by construction, so it correctly needs no assert. The distinction is reasoned, not
accidental.

## P5 — Unambiguous per-device framing
`device_sections` frames each device as `(device_id, section_version, len, bytes)` in bus
registration order, which prevents cross-device boundary ambiguity and version skew — the
exact discipline I wish the link applied at the blob/sections boundary (see Important #2). The
versioning per section is forward-compatible with device snapshot evolution.

## P6 — Strong, well-targeted test suite incl. live KVM
Five tests cover host-side chain math (determinism, chain-position sensitivity, per-component
perturbation across all six inputs, ascending panic) and live behavior (blob read-stability,
the normalized-TSC slot reflecting `vns`, and full-RAM byte-flip sensitivity). The byte-flip
offset `0x1F_F123` sits inside the 2 MiB live-test slot — confirmed in range. All 53 dh-vmm
lib tests pass on this host with /dev/kvm.

## P7 — Normalized TSC handled per the §8.1 restore rule
The blob carries `vns` in the IA32_TSC slot rather than the captured raw TSC, matching §8.1's
"we *write* vns on restore rather than trusting the captured value." The `vcpu_blob` live test
explicitly proves the slot tracks `vns` (42 vs 43 produce different blobs). This keeps host
TSC offset out of the portable state hash — correct for determinism.
