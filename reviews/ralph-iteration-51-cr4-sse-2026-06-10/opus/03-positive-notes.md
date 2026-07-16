# Positive notes (all verified live)

## CPUID mask is correct, math checks out, artifact is byte-identical

- Leaf 1 ECX clears `0x74209000` = MONITOR|PDCM|X2APIC|TSC_DEADLINE|RDRAND (prior)
  plus FMA(12)|XSAVE(26)|OSXSAVE(27)|AVX(28)|F16C(29), intersected with the host's
  supported `0x76fab223`. Computed independently — exact match.
- Leaf 7 EBX clears `0x00040020` = RDSEED(18) + AVX2(5). The AVX-512 group bits
  (16/17/21/26/27/28/30/31) are simply absent on this host's supported table, so they
  drop out of the diff — the mask constant is still correct and future-proof.
- Leaf 0xD fully zeroed across subleaves 0–4 (XSAVE enumeration), consistent with
  OSXSAVE-off and the leaf 6/0xA/0xB zeroing precedent.
- **Regenerated `dh-cli cpuid-diff` is byte-identical to the committed
  `docs/ops/cpuid-diff-infra-control.txt`** (`diff` clean).
- **Masked-table hash `f19610e179617f2c…` reproduced and invariant** across both
  available CPUs (`taskset -c 0` and `-c 1` — this box reports `nproc=2`, not 6;
  cross-core invariance holds on every core present).

## leaf 0xD zeroing does not break vCPU XSAVE ioctls

Confirmed by reasoning + live: `KVM_GET_XSAVE` returns a correct, host-sized 4096-byte
region with the live MXCSR even though the guest CPUID leaf 0xD is zeroed in the SET
table. The vCPU XSAVE ioctls are host-sized and do not consult guest CPUID for
sizing — zeroing guest-visible 0xD is safe. (This is also why XSAVE is the right
source to FIX the MXCSR Critical.)

## OSXSAVE masking in the table actually pins the guest-visible bit

The masked-table hash is run-to-run stable and the diff shows leaf 1 ECX bit 27
cleared. With no in-kernel irqchip and CR4.OSXSAVE off, the SET_CPUID2 table is what
the guest reads — the dynamic CR4.OSXSAVE mirror is irrelevant because it's off.

## sse_probe guest is sound

- `objdump` confirms `vec_a/vec_b/vec_c/result` land 16-byte aligned (0x100090,
  0x1000a0, 0x1000b0, 0x1040d0) — `movdqa` won't `#GP` on alignment.
- GP-verify constants match the SSE math: lane0 `0x33…3a` (= 0x1111…^0x2222… + 7),
  lane1 `0xcc…d5` (= 0x4444…^0x8888… + 9). Computed independently — exact.
- `'V'`=0x56 success / `'v'`=0x76 fail bytes correct.
- **Live: the test passes — guest boots to HLT and emits `"V"`**, proving CR4.OSFXSR
  genuinely enables SSE2 (without it the first `movdqa` `#UD`s into a triple fault).

## XMM and x87 are faithfully in the hash

- Live: guest writes `0xDEADBEEFCAFEBABE:0x0123456789ABCDEF` to xmm3; `KVM_GET_FPU`
  returns the exact 16 bytes; `hash.rs:257-259` serializes all 16 XMM regs.
- x87 `fpr`/`fcw`/`fsw`/`ftwx`/`last_*` serialized at `hash.rs:248-256`.
- (MXCSR is the lone exception — see the Critical.)

## Determinism battery fully green; the 997 invariant is untouched

Ran live (kernel 6.8.0-124):
- `regression`: 1e9 instructions twice → equal final hash; 1e7 twice → equal.
- `if0_deferral`: masked-window deferral identical across 100 runs.
- `landing_precision`: 10,000 random targets, zero overshoots; REP-string boundaries
  never land mid-REP.
- `counting_semantics`: single-step attribution sums to **997**, replay-identical;
  landing across an MMIO write does not free-run (icount==20).
- `m1_acceptance`, `timer_determinism` (100 runs identical), `skid_gate`, `gate`.
- Full workspace `cargo test`: all green. The CR4 change did not perturb retirement
  counting.

## Builds clean both arches

`cargo clippy --workspace --all-targets` clean on x86_64 and on
`aarch64-unknown-linux-gnu` (with the documented clang/llvm-ar env). Tree clean after
all experiments.
