# Action items

### Critical

- [ ] **Fix MXCSR capture in the §8.1 state hash.** `crates/dh-vmm/src/hash.rs`
  currently sources `fpu.mxcsr` from `KVM_GET_FPU` (line 182, serialized line 260),
  which on kernel 6.8 returns `0x0000` regardless of the guest's live MXCSR
  (live-proven: guest=0x7F80, GET_FPU=0x0000, GET_XSAVE=0x7F80). The guest's SSE
  rounding mode / exception masks thus escape the hash — two replays differing only in
  rounding mode collide. Source MXCSR from `KVM_GET_XSAVE` (legacy FXSAVE region,
  byte offset 24 / `region[6]`) instead, keeping x87/XMM as-is. Self-contained repro:
  boot a guest that does `ldmxcsr [0x7F80]; hlt`, read both ioctls, observe the
  divergence.

- [ ] **Add the regression guard.** A live `hash.rs` test that boots a guest setting
  MXCSR=0x1F80 vs 0x7F80 and asserts `canonical_vcpu_blob` differs. It must fail
  before the fix and pass after — that failure IS the proof the hole was real.

### Important

- [ ] **Correct the boot.rs / hash.rs comments** (`boot.rs:234-240`,
  `hash.rs:11-14,247`) that assert guest FP state "is exactly what `KVM_GET_FPU`
  captures." Until/unless MXCSR is sourced from XSAVE, this is false; after the fix,
  name the actual capture source and drop the GET_FPU credit.

- [ ] **Document the OSXMMEXCPT + no-IDT triple-fault edge** in ARCH §2.3: an unmasked
  SSE FP fault becomes `#XM` → Shutdown (live-verified). Safe today only because the
  guest's boot MXCSR is 0x1F80 (all masked); a guest that unmasks (e.g. C
  `feenableexcept`) triple-faults. State the invariant that determinism guests keep
  SSE exceptions masked.

### Suggestions

- [ ] Set `CR0.MP` (boot.rs:242 → `0x8000_0023`) to match the canonical FP-code CR0
  for future compiled guests. Benign today; removes a future surprise.
- [ ] `bd remember` the GET_FPU-returns-0-MXCSR-on-6.8 fact for the M4 XSAVE-codec
  author.
- [ ] (Optional) guest-side CPUID assertion in `sse_probe` for OSXSAVE/AVX masking —
  belt-and-suspenders; the SET-table pin is already authoritative. Skipping is fine.
