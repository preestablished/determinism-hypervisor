# XSAVE Canonicalization — Independent Review (2nd reviewer)

- **Branch:** `ralph/iteration-68-xsave-canonicalization` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus (2nd reviewer)
- **Bead:** ec4 — XSAVE canonicalization (risk R7)
- **Diff:** 3 files, +285 / −4 (`crates/dh-vmm/src/xsave.rs` new 265 lines; `hash.rs` +21; `lib.rs` +3)

## Summary

Adds `crates/dh-vmm/src/xsave.rs`: a pure byte transform that, for every XSAVE
component whose `XSTATE_BV` bit is clear, zeroes that component's area so
logically-equal vCPU state hashes equal (ARCH §8.1, risk R7). The module is
ungated (compiles/unit-tests on aarch64); only its CPUID-derived layout and
`KVM_GET_XSAVE` wiring are x86-gated. `hash.rs::canonical_vcpu_blob` now appends
the full canonicalized XSAVE area to the preimage, resolving the Phase-1 deferral
for the hash path. MXCSR/MXCSR_MASK `[24,32)` and legacy reserved `[416,512)` are
deliberately left untouched (not component-governed). Bounds are checked loudly
(`TooShort`, `ComponentOutOfBounds`).

## Live verification on this box (x86_64, /dev/kvm present)

All experiments run on the actual host; scratch artifacts reverted.

1. **Host CPUID 0xD layout** — XCR0=0x1f (x87+SSE+AVX+MPX BNDREGS+BNDCSR), area
   size 1088 B. AVX bit2 → offset 576 size 256 (matches the test fixture exactly);
   BNDREGS bit3 → 960/64; BNDCSR bit4 → 1024/64. All fit in 4096.
2. **R7 hazard is LIVE today (not merely defensive).** Fresh vCPU returns
   `XSTATE_BV=0` (x87 + SSE bits CLEAR), yet `KVM_GET_XSAVE` leaves the x87 area
   nonzero: `FCW=0x037f` at bytes [0,1] and reserved byte [464]=0x1f are present.
   With bit0 clear, the un-canonicalized blob would hash that `0x037f` (default
   x87 control word) — canonicalization zeroes it. Two back-to-back reads are
   byte-identical, so the live stability assertion is sound (no flake).
3. **GET_XSAVE vs GET_XSAVE2:** code uses the fixed-size `get_xsave()`
   (region `[u32;1024]` = 4096 B). `KVM_CAP_XSAVE2` reports 4096 on this host;
   guest CPUID 0xD is fully zeroed (OSXSAVE off) so the guest never grows XCR0.
   Fine here; latent gap on AVX-512+AMX hosts — see 01 (Important, file-worthy
   for 55f). Mitigation: on such a host `host_component_layout()` would enumerate
   an offset ≥ 4096 and `canonicalize` fails loudly (`ComponentOutOfBounds`),
   so the system fails closed rather than silently wrong.
4. **Stability under no-run:** verified two GET_XSAVE calls are byte-identical
   with no guest execution; the live test's equality assertion is not flaky.
5. **Pinned hashes:** no test pins an absolute state-hash/chain value derived from
   real `canonical_vcpu_blob`. The dh-snapshot/dh-inputlog golden BLAKE3 constants
   use synthetic VCPU payloads — the preimage change does not break them. Verified.
6. **aarch64:** `cargo check -p dh-vmm --target aarch64-unknown-linux-gnu` passes
   — `xsave.rs` compiles (pure core; x86 helpers gated).
7. **Tests:** `cargo test -p dh-vmm --lib` → 91 passed (incl. live
   `live_xsave_canonicalizes_and_is_stable`, `host_layout_is_sane`).
   `cargo test -p determinism-tests --test regression` → 2 passed
   (10M and 1B instruction runs produce equal final hash through the live,
   canonicalized hash path). `cargo clippy -p dh-vmm --lib` clean.

## Verdict

**APPROVE.** Correct, well-tested, well-scoped. The transform is sound, the R7
risk was genuinely live (not just defensive), and every quality gate passes on
real hardware. One Important latent issue (GET_XSAVE truncation on big hosts) is
correctly out of this bead's scope but should be filed for 55f; the fail-closed
behavior makes it non-blocking.

## Stats

| Severity   | Count |
|------------|-------|
| Critical   | 0     |
| Important  | 1     |
| Suggestion | 3     |
| Positive   | 5     |
