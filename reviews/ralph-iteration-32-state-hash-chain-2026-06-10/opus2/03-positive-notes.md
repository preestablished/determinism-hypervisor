# Positive Notes

### P-1. Field-by-field LE serialization, never raw struct memory

`canonical_vcpu_blob` and `seg()` serialize every field explicitly (`to_le_bytes`, individual `push`
for u8 fields) rather than transmuting the `kvm_*` structs. This is exactly right: the C structs carry
padding bytes that are *not* part of machine state and would otherwise inject host-uninitialized-memory
nondeterminism into the hash. The docstring (hash.rs:158-160) calls this out explicitly. This is the
single most important correctness property of a state hash and it's done correctly throughout.

### P-2. The live tests actually run on this host — capture is verified, not assumed

Both `vcpu_blob_is_stable_across_reads_live` and `final_link_sees_guest_ram_live` executed (0 skip
lines). They prove on real KVM: (a) capture is read-stable (two reads of an idle vCPU hash equal),
(b) the normalized-TSC slot tracks `vns` (43 ≠ 42), (c) the full RAM walk sees guest memory and a
single flipped byte at 0x1F_F123 perturbs the hash. The `get_msrs`-returns-14 invariant (incl.
SPEC_CTRL) is implicitly proven because the blob builder errors out otherwise and both tests pass.

### P-3. Normalized TSC handling matches §8.1 restore rule and §2.2 MSR policy

IA32_TSC (0x10) is deliberately excluded from the GET list and its blob slot is filled with the
normalized `vns` (hash.rs:298-301), matching ARCH §8.1 ("we *write* vns on restore rather than trusting
the captured value") and consistent with `msr.rs`'s deliberate denial of IA32_TSC (the documented R6
host-leak: live 0x19ebc vs 0x18702). Hashing the raw TSC would make every run-twice-compare fail. This
is the subtle correctness call and it's right.

### P-4. Strict ascending-page assertion with a panic, not a guest-influenced error path

`push_link` asserts `bytes.len() == PAGE_SIZE` and strictly-ascending indices (hash.rs:99-104), with a
docstring stating these are caller bugs, not guest-influenced. `is_none_or` is the idiomatic ascending
check. The `out_of_order_pages_panic` test pins it. Correct severity classification — an out-of-order
page is a programming error, and panicking is the right response (vs. silently hashing a non-canonical
order and producing a stable-but-wrong hash).

### P-5. XSAVE / SREGS2 deferral is documented at exactly the right altitude

The module and inline comments state plainly that XSAVE proper (AVX/YMM) is M4 (hash.rs:233, 649-653
of ARCH) and that SREGS2's PDPTR extension only matters for PAE-without-LMA guests not present on this
machine. Combined with the nanokernel guests using no SSE/AVX (CR4 gated), the FPU-only capture is
Phase-1-sound, and the doc says so explicitly enough to defend the scope. The "M4 extends, never
replaces — same harvest order" framing is honored in the link layout.

### P-6. Test suite covers determinism + per-component sensitivity thoroughly

`links_chain_and_every_input_matters` perturbs every input independently (vcpu bytes, page idx, page
content, icount, vns) and asserts each changes the hash, plus that chain position matters (a second
identical link diverges). `h0_is_deterministic_and_input_sensitive` covers H_0. This is the right
shape for a hash module — both directions (equal in ⇒ equal out, and each field out ⇒ different out).

### P-7. Single reused page buffer in the hot walk

`push_final_link` allocates one `[0u8; PAGE_SIZE]` and reuses it across all pages (hash.rs:128-135),
avoiding 832K allocations on the max-size walk. Small but correct given this runs while the vCPU is
paused and on the latency-sensitive snapshot boundary.
