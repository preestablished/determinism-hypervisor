# Suggestions (non-blocking)

### S1 — XCOMP_BV is preserved as-read but never validated to be 0 in the canonical blob

`crates/dh-vmm/src/xsave.rs:84-86,136` reads `xcomp_bv`, rejects only bit 63 (compacted form),
then copies the full `xcomp_bv` verbatim into the canonical blob. For standard-format KVM output
XCOMP_BV is 0, so this is fine today. But if a host ever returned a standard-format area with a
nonzero XCOMP_BV low bits (some kernels stash the supported-feature mask there), that variance
would flow straight into the hash. Low risk, but since the canonical form is meant to be
*derived from logical state only*, consider canonicalizing XCOMP_BV to 0 in the rebuilt blob (it
is not logical guest state in standard form), or asserting it equals 0 with a loud error. At
minimum add a one-line comment at line 136 noting the deliberate pass-through and why it is safe.

### S2 — `is_x87_init` indexes `area` without a length precondition comment

`is_x87_init` (line 143) reads `area[0..2]`, `area[2..24]`, `area[32..160]`. It is only ever
called after `xstate_bv()` has confirmed `area.len() >= 576`, so the indexing cannot panic — but
that precondition is implicit. A `debug_assert!(area.len() >= XSAVE_MIN_LEN)` at the top of
`is_x87_init`, or a doc line stating "caller guarantees len >= 576", makes the safety local and
survives refactors that might call it elsewhere.

### S3 — `keep` Vec allocation per call is avoidable on the hot hash path

`canonicalize` allocates a `Vec<(usize,usize)>` (`keep`) and a full `vec![0u8; area.len()]`
(`canon`) on every call. This runs once per state-hash (every hash point). For a determinism
hypervisor the correctness is paramount and this is fine, but if hashing ever shows up in a
profile, the rebuild can be done in place (zero the non-kept ranges instead of allocating a
shadow buffer) or `keep` can be a fixed-size `SmallVec`/array (max components is small and
bounded). Pure micro-optimization — do not do this speculatively; note it for if/when it matters.

### S4 — Add an explicit test that a non-init *extended* component (set bit, non-zero area) keeps its bit

The new test `init_state_normalizes_to_clear_bit_regardless_of_encoding` covers the *non-init x87*
case (FCW != 0x037F keeps bit 0), which is excellent. It does not have the symmetric extended-bit
case: an extended component with a set bit and a *non-zero* area must keep its bit and its bytes.
`extended_components_follow_the_table` (the "AVX set" case, line 295) does exercise area survival
with `0xEE` fill, so the behavior is covered — but a one-assert addition that the *bit* survives
(`assert_eq!(xstate_bv(&a).unwrap(), 0b111)` after the AVX-set canonicalize) would pin the
init-normalization boundary for extended bits as tightly as it is pinned for x87. Cheap insurance
against an I1-style regression.
