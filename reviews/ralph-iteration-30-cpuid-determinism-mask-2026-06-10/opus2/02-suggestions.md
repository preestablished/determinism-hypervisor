# Suggestions (non-blocking)

### S-1. Add a hosted-lane unit test using a synthetic `CpuId::from_entries` — all three current tests skip without `/dev/kvm`

Every test in `cpuid.rs` early-returns when `kvm_usable()` is false, so on any CI lane or dev box without `/dev/kvm` the entire module has **zero** coverage — the mask logic, the hash, and the PV-leaf `retain` are completely untested off-box. The mask logic does not need a live KVM: build a small synthetic `CpuId::from_entries(&[...])` with hand-set leaf 1 / 6 / 7 / 0xA / 0x80000001 / 0x80000007 entries plus a `0x40000000` PV leaf, run `mask_in_place`, and assert:

- every named bit is cleared,
- leaf 0xA and leaf 6 are fully zeroed,
- the `0x40000000` entry is gone and `nent` shrank by one (proves `retain` semantics),
- `cpuid_table_hash` of the synthetic-then-reversed table is equal (order-independence without needing KVM),
- a one-bit difference in any field changes the hash (mask-sensitivity without the "RDRAND exists on every lab host" assumption — see S-2).

This is the single highest-value follow-up: it makes the mask logic testable everywhere and decouples correctness from box availability.

### S-2. The mask-sensitivity assertion bakes in "RDRAND exists on every lab-class host"

`hash_is_order_independent_and_mask_sensitive_live` asserts `hash(supported) != hash(masked)`. The inline comment admits this rests on RDRAND being present. That holds on lab hardware but is a latent flake on an exotic/virtualized host with no maskable bits set (e.g. a stripped nested-virt guest where KVM_GET_SUPPORTED_CPUID happens to advertise none of the masked bits). The synthetic test in S-1 removes the assumption entirely for the off-box lane; for the live test, consider asserting that at least one *expected* clear happened (e.g. PV leaves removed, which is true on any KVM host) rather than relying on a flag-set difference.

### S-3. `cpuid-diff` is numeric-only — no bit names — which hurts the M1 acceptance-review usability the tool exists for

The module doc says `cpuid-diff` is "for the M1 acceptance review." A reviewer reading `leaf 0x00000001.0 ecx: ... (cleared 0x40208000)` has to hand-decode `0x40208000` → RDRAND|x2APIC|PDCM. Since the mask constants (`L1_ECX_RDRAND`, etc.) already carry exactly the human meaning, consider annotating the cleared mask with the constant names (a small `(bit, name)` table keyed per leaf/register). Purely a UX improvement for the acceptance reviewer; the numeric form is fine for machine diffing. Suggestion only.

### S-4. Hash preimage does not domain-separate leaf boundaries — fine today, brittle if the entry struct grows

`cpuid_table_hash` concatenates fixed-width LE fields with no length prefix or separator. That is unambiguous **only because** every entry serializes to exactly 28 bytes (7 × u32) and the count is implied by total length. It is safe today. If a future change ever serializes a variable-length field (unlikely for CPUID, but the `flags`/padding could change across kvm-bindings majors), the lack of framing becomes a collision risk. Cheap hardening: prepend `entries.len() as u32` to the preimage. Optional.

### S-5. Consider asserting the `(7, 0)` arm only fires for subleaf 0 — and document why other leaf-7 subleaves are untouched

The match arm `(7, 0)` correctly restricts RDSEED clearing to subleaf 0. But leaf 7 has subleaves 1+ on newer CPUs (subleaf 1 EAX carries AVX-VNNI, etc.). None of those carry masked-class bits today, so untouched is correct, but a one-line comment ("leaf 7 subleaves ≥1 carry no determinism-class bits today; revisit if AVX512/AMX masking lands per §7.2") would make the intent explicit and tie back to the AVX512 "lowest common denominator" note in §7.2.
