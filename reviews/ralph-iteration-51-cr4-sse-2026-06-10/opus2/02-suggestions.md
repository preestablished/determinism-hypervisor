# Suggestions

### S1. Extend sse_probe to exercise the float path (MXCSR) and FXSAVE/FXRSTOR

The probe runs only SSE2 **integer** ops (`movdqa`/`pxor`/`paddq`). That proves OSFXSR
gates legacy-SSE access, but it never touches the two places where determinism risk
*actually* lives for SSE:

1. **The float path / MXCSR.** Rounding mode, denormals-are-zero, flush-to-zero, and
   the exception-status flags all live in MXCSR and are part of the §8.1 hash. A
   `movaps`/`addps`/`mulps` sequence plus a read-back would exercise the rounding/MXCSR
   surface that the integer ops bypass entirely. If a future change perturbed the
   initial MXCSR (e.g. a different default than 0x1F80), the integer probe would not
   notice.
2. **FXSAVE/FXRSTOR.** OSFXSR's *other half* is that it enables FXSAVE/FXRSTOR. The
   probe could `fxsave` to a 512-byte aligned buffer and assert a known word (e.g. the
   MXCSR field at offset 24, or an xmm lane) to prove the save area is what we expect —
   this is exactly the state shape that M4 restore will have to round-trip.

Suggested, not required: the determinism payoff is real (the float/MXCSR state is in
the hash but currently unexercised by any guest), but the integer probe already
discharges the immediate "does OSFXSR take?" question this bead set out to answer.

### S2. Add the new mask bits to the live assertion test (pairs with I1)

Concretely, in `mask_clears_the_documented_bits_live`:
```rust
(1, _) => {
    // ... existing asserts ...
    assert_eq!(e.ecx & (L1_ECX_FMA | L1_ECX_XSAVE | L1_ECX_OSXSAVE
                         | L1_ECX_AVX | L1_ECX_F16C), 0, "XSAVE/AVX family");
}
(7, 0) => {
    // ... existing ...
    assert_eq!(e.ebx & (L7_EBX_AVX2 | L7_EBX_AVX512_GROUP), 0, "AVX2/AVX512");
}
(0xD, _) => assert_eq!((e.eax, e.ebx, e.ecx, e.edx), (0,0,0,0), "leaf 0xD zeroed"),
```
This turns the I1 gap into a guarded invariant at near-zero cost.

### S3. Document the artifact's non-byte-stability inline

A one-line header comment in `docs/ops/cpuid-diff-infra-control.txt` noting that the
`leaf 0x01.0 ebx` and `leaf 0x0B.0 edx` *supported-side* values are host-LP-placement
dependent (and that only the `masked table hash` line is stable / authoritative) would
save the next reviewer the re-derivation I just did. Optional.

### S4. Tiny precision note on the AVX-512 group naming

The doc-comment on `L7_EBX_AVX512_GROUP` lists "F/DQ/IFMA/PF/ER/CD/BW/VL" but the bit
set is `16,17,21,26,27,28,30,31`. Bit 21 is AVX512_IFMA, 16/17 are F/DQ, 28 is CD,
26/27 are AVX512PF/ER, 30/31 are BW/VL — that matches the comment, good. No change
needed; just confirming the enumeration is accurate for anyone auditing it. (PF/ER are
Knights-Landing-only and will never appear on a Xeon-SP/Core fleet host, but masking a
never-set bit is harmless.)
