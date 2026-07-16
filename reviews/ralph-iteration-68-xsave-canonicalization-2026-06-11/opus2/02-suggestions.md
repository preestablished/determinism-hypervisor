# Suggestions (non-blocking)

### S-1 — Live test should assert canonicalization actually CHANGED bytes

`xsave::live_tests::live_xsave_canonicalizes_and_is_stable` asserts the clear
components are zero post-transform and that two reads are stable — but it never
asserts the transform *did something*. My live probe proved a fresh vCPU returns
`FCW=0x037f` (nonzero) in the clear x87 area, so on this box canonicalization is
load-bearing, not a no-op. A regression that made `canonicalize` a no-op (e.g. a
bad XSTATE_BV read) would still pass this test. Consider snapshotting the
pre-transform bytes and asserting `pre != post` when `bv & 3 != 3`:

```rust
let pre = a.clone();
canonicalize(&mut a, &layout).unwrap();
if bv & 1 == 0 && pre[0..2] != [0, 0] {
    assert_ne!(pre, a, "canonicalization must have zeroed live garbage");
}
```

`crates/dh-vmm/src/xsave.rs` (live_tests).

### S-2 — Document the "init state ≠ all-zeros" subtlety in the module doc

Zeroing a clear component is correct for *equality* (deterministic, collision-free
across logically-equal states), but a reader steeped in the SDM may expect the
canonical form to be the *architectural init value* (x87 init = FCW 0x037f / FTW
0xffff, MXCSR 0x1f80), not all-zeros. The choice is fine — it is a hash preimage,
not a restorable area — but one sentence in the `canonicalize` doc stating "clear
components are zeroed (not set to architectural init values); this is a hash
preimage, never restored" would preempt the question. Pairs with the existing
MXCSR note. `crates/dh-vmm/src/xsave.rs` (doc on `canonicalize`).

### S-3 — Reduce the per-call CPUID cost / two allocations in `canonical_vcpu_blob`

`canonical_vcpu_blob` calls `host_component_layout()` on every invocation, which
re-runs CPUID 0xD subleaves each time, and builds a fresh `Vec<u8>` via
`flat_map(...).collect()` plus the layout `Vec`. On the hot hash path (1B-instruction
regression run still passes in 4.3s, so not urgent) the layout is host-invariant —
a `OnceLock<Vec<XsaveComponent>>` would make it free after first call:

```rust
static HOST_XSAVE_LAYOUT: std::sync::OnceLock<Vec<XsaveComponent>> = std::sync::OnceLock::new();
let layout = HOST_XSAVE_LAYOUT.get_or_init(host_component_layout);
```

Non-blocking; only matters if hash points get dense. `crates/dh-vmm/src/xsave.rs`
+ `crates/dh-vmm/src/hash.rs:278`.
