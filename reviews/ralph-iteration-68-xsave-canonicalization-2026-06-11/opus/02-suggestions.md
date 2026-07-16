# Suggestions (non-blocking)

## S1 — Validate XCOMP_BV == 0 (compact-format guard)

`xsave.rs` assumes the **standard** format: legacy areas at fixed offsets and the extended table's `offset` (CPUID EBX) being absolute. KVM's `KVM_GET_XSAVE` always returns standard format (XCOMP_BV = 0), and the doc comment states this. But if a future `KVM_GET_XSAVE2`/compact-format path ever feeds this function, EBX offsets would be wrong and clear-bit zeroing would land on the wrong bytes. Cheap defense: in `canonicalize` (or a debug assert), read XCOMP_BV at `[520,528)` and reject (or `debug_assert!`) non-zero. The header is already in-bounds (XSAVE_MIN_LEN = 576). This converts a silent-wrong-area risk into a loud error for the 55f reuse.

```rust
let xcomp = u64::from_le_bytes(area[520..528].try_into().expect("checked length"));
debug_assert!(xcomp & (1 << 63) == 0, "compact XSAVE format not supported by this transform");
```

(The MSB of XCOMP_BV is the compaction-enable bit; checking the whole word `!= 0` is also fine.)

## S2 — `host_layout_is_sane` could assert table non-overlap / in-area

`live_tests::host_layout_is_sane` checks `bit >= 2`, `offset >= 576`, `size > 0`. Since this same table drives `fill(0)` ranges that 55f will reuse on real SET data, consider also asserting the entries don't overlap each other and (when a real area length is known) `offset + size <= area_len`. Not required — `canonicalize` already bounds-checks against the actual buffer — but it would catch a malformed host table closer to the source.

## S3 — `to_le_bytes` round-trip allocates twice in the hash hot path

`hash.rs:277` rebuilds `xsave_bytes` from `xsave.region` via `flat_map(to_le_bytes).collect()`, then `canonicalize` mutates it, then it's copied into `out`. The `region` is already a `[u32; N]`; on little-endian hosts (the only target) this is a memcpy-shaped transform. Fine as-is for clarity, but if the blob is ever built per-instruction-boundary at high frequency, consider zeroing in place over a `bytemuck`/`as_bytes` view of `region` to drop one allocation. Purely an efficiency note; correctness is unaffected.

## S4 — Note the intentional MXCSR double-count where it is hashed, not only where it's defined

The duplication (MXCSR hashed once as `region[6]` at `hash.rs:268`, once inside the canonical area at `hash.rs:280`) is explained in the comment at the canonical-area site and is harmless (it cannot mask a difference — two copies of the same field strictly preserve sensitivity). The `region[6]` field predates this change; a one-line back-reference at line 268 ("also covered by the canonical area below; kept pinned per iteration-51") would save the next reader the cross-check. Optional.
