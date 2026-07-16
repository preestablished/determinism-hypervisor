# Suggestions

## S1 — Remove the stray `let _ = &mut msrs;` in `set_tsc_value_msr`

**File:** `crates/dh-vmm/src/tsc.rs:105`

```rust
let n = vcpu.set_msrs(&msrs).map_err(...)?;
let _ = &mut msrs;          // <-- leftover; does nothing useful
if n != 1 { ... }
```

`set_msrs` takes `&msrs` (an immutable borrow that ends at the call). The subsequent
`let _ = &mut msrs;` takes a throwaway mutable borrow of a value that is never used again —
it is dead code, almost certainly a leftover from an earlier draft (perhaps an attempt to
silence an "unused mut" lint or to model a kernel write-back). `KVM_SET_MSRS` does **not**
write back through the `Msrs` buffer (it returns the count as the ioctl return value, which
`kvm-ioctls` surfaces as `n`), so there is nothing to observe here. The binding can be
`let msrs = ...` (drop the `mut`) and this line deleted. Clippy is currently silent only
because the `&mut` borrow "uses" the `mut`. Removing both is cleaner and removes a false
hint that the kernel mutates the buffer.

## S2 — Decision doc: state the 1 GHz virtual-TSC convention next to `offset = vns − host_tsc_at_resume`

**File:** `docs/decisions/tsc-alignment.md:230-231`

> Restore computes `offset = vns − host_tsc_at_resume` and issues one `KVM_SET_DEVICE_ATTR`.

`vns` is virtual **nanoseconds**; a TSC offset is in **TSC ticks**. The subtraction is only
dimensionally valid because ARCHITECTURE.md:341-343 fixes the default
`clock_num=1, clock_den=1` → "deterministic 1 GHz", i.e. **guest TSC ticks at 1 GHz so 1
tick == 1 vns (1:1)**. That convention is load-bearing for this formula but is not stated in
the decision doc, and it interacts with the fact that CPUID leaf 0x15 is zeroed (iter-30) —
the guest cannot calibrate its own TSC frequency, so the 1 GHz convention is *imposed*, not
*advertised*. Add one sentence, e.g.:

> Units: with the default `clock_num=clock_den=1` (ARCH §… "deterministic 1 GHz"), guest TSC
> advances 1 tick per virtual nanosecond, so `vns` and guest-TSC ticks are 1:1 and the
> subtraction is well-typed. For a non-unity clock ratio, scale `vns` by `clock_den/clock_num`
> first.

This is a doc clarity fix, not a code defect — the M4 codec just needs to know the unit
assumption it is inheriting.

## S3 — Refresh the decision-doc benchmark numbers or mark them representative

**File:** `docs/decisions/tsc-alignment.md:213-218`

The doc records 986 / 1591 ns/call; this review's live run measured **1117 / 1489**. The
qualitative ordering and the ~0.5%/4.8 ms-per-guest-second conclusion are unaffected by the
run-to-run variance, so this is not a blocker. Either re-capture once after the Critical fix
lands (the numbers should not change — the fix only touches GET, which is off the benchmark
loop) and update, or add "(representative single-run; ±~15% run-to-run on the lab box)" so a
future reader is not surprised by drift.

## S4 — Bead file-scope note: landed in `tsc.rs`, not `run*.rs`

The bead 3np says `Files: crates/dh-vmm/src/run*, docs/**`. The mechanism landed in a new
`tsc.rs` module instead. This is the **better** call — TSC alignment is a cohesive concern
that restore (`run.rs`) will *call into*, and keeping it out of the run hot-path module is
cleaner. No change needed; just note in the bead/commit that the file location intentionally
deviates from the bead text so the deviation is auditable. (M4 will wire `set_tsc_offset`
into the restore path in `run.rs` per §8.3 — that is the future consumer.)
