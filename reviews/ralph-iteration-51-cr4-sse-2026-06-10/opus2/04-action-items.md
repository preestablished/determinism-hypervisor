# Action items

### Critical

_None._ Nothing blocks merge.

### Important

- **A1 — Guard the new mask bits with a live assertion (addresses I1).** Extend
  `mask_clears_the_documented_bits_live` in `crates/dh-vmm/src/cpuid.rs` so it asserts
  the bits this iteration added are cleared:
  - leaf 1 ECX: `L1_ECX_FMA | L1_ECX_XSAVE | L1_ECX_OSXSAVE | L1_ECX_AVX | L1_ECX_F16C` → 0
  - leaf 7 EBX: `L7_EBX_AVX2 | L7_EBX_AVX512_GROUP` → 0
  - leaf 0xD (all subleaves): `(eax,ebx,ecx,edx) == (0,0,0,0)`
  Leaf 0xD is the load-bearing one on this host (18 live diff lines); without an
  assertion the determinism guarantee is unprotected against future refactors. Cost is
  near-zero; the live test already iterates the masked table.

- **A2 — Record the cpuid-diff artifact's non-byte-stability (addresses I2).** The
  committed `docs/ops/cpuid-diff-infra-control.txt` does NOT byte-match a fresh
  `dh-cli cpuid-diff` run: the `leaf 0x01.0 ebx` and `leaf 0x0B.0 edx` *supported-side*
  lines vary with host-LP placement (initial APIC / x2APIC ID), exactly as root-caused
  in iteration-48. The masked-table hash (`f19610e1…`) IS stable. No code change needed,
  but if this artifact is ever promoted to a CI gate, compare on the `masked table hash`
  line only (or canonicalize APIC/x2APIC fields out of the supported side first). At
  minimum, add a header comment to the artifact (see S3).

### Suggestions

- **S1 — Extend sse_probe to cover the float/MXCSR path and FXSAVE/FXRSTOR.** The
  current probe is SSE2-integer-only; rounding/MXCSR (which IS in the §8.1 hash) and
  the FXSAVE half of OSFXSR are unexercised. A `movaps`/`addps` + `fxsave`-and-check
  would close the gap. Optional; the integer probe answers the immediate bead question.

- **S3 — Inline comment on the artifact** noting the supported-side APIC/x2APIC lines
  are placement-dependent and only the hash line is authoritative.

- **S4 — (no-op)** AVX-512 group const comment verified accurate; no change.

### Re-baseline status

`ci/determinism-class.lock` pins host identity only (cpu vendor/family/model/stepping/
brand, microcode, kernel) — **NOT** the CPUID/masked-table hash. The old hash
`4dac1b7a…` lives only in prior review docs, never in a live gate. **This change does
NOT trigger the re-baseline procedure.**

### Verification performed (all green)

- `cargo test -p dh-cli --test boot_hello sse_probe_proves_osfxsr` → PASS (`V`)
- `cargo test -p determinism-tests --test counting_semantics` → PASS (997 still pinned)
- `cargo test -p dh-vmm --lib cpuid` → 3 PASS
- `cargo test --workspace` → all PASS, 0 failures
- `cargo clippy --workspace --all-targets` (x86_64) → clean
- `cargo clippy --workspace --all-targets --target aarch64-unknown-linux-gnu`
  (CC=clang, CFLAGS=--target=… -isystem /tmp/a64inc, AR=llvm-ar-18) → clean
- `dh-cli cpuid-diff` x5 → masked hash `f19610e1…` stable, matches committed artifact
- `git status` → tree clean after all runs
