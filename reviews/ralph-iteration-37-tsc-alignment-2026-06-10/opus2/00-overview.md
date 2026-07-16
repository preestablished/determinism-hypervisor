# TSC alignment (bead 3np, iter-37) — second-reviewer overview

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-37-tsc-alignment` vs `main`
- **Scope:** `crates/dh-vmm/src/tsc.rs`, `docs/decisions/tsc-alignment.md`, lib wiring
- **Environment:** lab box, kernel 6.8, `/dev/kvm` rw — all tests run live, both `debug` and `release`.

## Verdict: REQUEST CHANGES (one Critical)

The **decision is correct and well-supported** — the `KVM_VCPU_TSC_OFFSET` attribute is the right
M4 restore mechanism, the heuristic-hazard reasoning is sound, and the `_IOW` direction of
`KVM_GET_DEVICE_ATTR` is correctly identified. The raw-ioctl approach is justified: kvm-ioctls 0.24
genuinely `#[cfg(target_arch = "aarch64")]`-gates `set_device_attr`/`has_device_attr` and ships **no**
`get_device_attr` at all (verified in the vendored source).

But `get_tsc_offset` has **latent undefined behavior that already manifests as a wrong result in
release builds**: the function returns `0` instead of the kernel-written offset under `-O`, failing its
own round-trip assertion. The committed live test passes only because CI runs it in `debug`. This must be
fixed before M4 wires restore. One Critical, one Important, three Suggestions.

## Headline adjudications (lead items)

1. **Aliasing soundness (CONFIRMED UB, Critical).** `get_tsc_offset` points `attr.addr` at a `let raw =
   0u64` local via a **shared** `&u64` (`p as *const u64 as u64`). The kernel writes 8 bytes there, but
   Rust's model sees only a shared borrow of a non-`UnsafeCell` local that is never written, so LLVM
   constant-folds the return to `0`. **Reproduced live:** `cargo test --release` → `left: 0, right:
   -123456789`. `debug` passes. **Minimal sound fix verified live:** `let mut raw = 0u64; addr:
   std::ptr::addr_of_mut!(raw) as u64;` → release round-trip passes. (kvm-ioctls itself routes
   kernel-written ioctls through `&mut` / `ioctl_with_mut_ref` for exactly this reason.)

2. **Doc formula (NO unit bug — formula is correct under the spec; caveat missing, Important).** I
   examined the worry that `offset = vns − host_tsc_at_resume` needs a `× host_freq` conversion. It does
   **not**, because ARCH §4.1 defines `vns = icount·clock_num/clock_den` (default 1:1 = "deterministic 1
   GHz") and §8.3 restore literally writes `IA32_TSC ← vns` — i.e. the architecture *defines the virtual
   TSC unit to be vns* (a 1 ns tick). Under that definition guest-TSC and vns share one unit, so the
   formula yields `guest_tsc == vns` at the resume instant with no frequency factor. The real, omittable
   subtlety is **post-resume drift**: `host_tsc` advances at the host rate (~3 GHz on the Coffee Lake box)
   while `vns` advances at the rational rate, so the guest TSC diverges from vns immediately after resume —
   which is *exactly* the §4 defense-4 position ("approximately virtual ... drifts only between exits";
   guests read time only via pv-clock). The formula is right; the doc should state the drift caveat so an
   M4 implementer does not "fix" a non-bug.

## Stats

| | count |
|---|---|
| Critical | 1 |
| Important | 1 |
| Suggestions | 3 |
| Positive notes | 5 |

## What I verified live

- `has_tsc_offset_attr` → true on this kernel; offset round-trips bit-exactly in **debug**.
- **Release round-trip FAILS** (`get_tsc_offset` returns 0) — reproduced 4×, deterministic.
- `addr_of_mut!(raw)` fix → release round-trip **passes**.
- Benchmark fairness: hoisting the `Msrs` allocation does **not** narrow the gap (alloc ≈ hoist ≈
  1.10 µs; offset-attr ≈ 0.93 µs). The allocation is not the cost; the ioctl/KVM path is.
- The doc's `1591 ns` MSR figure is not reproducible here (~1100 ns repeatably); decision stands but the
  table overstates the gap ~3.5×.
- `cargo clippy --all-targets -D warnings` is clean and does **not** catch the UB (it is opsem, not a
  lint); CI has no release lane and no Miri.
