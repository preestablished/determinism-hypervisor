# Suggestions (non-blocking)

## S1 — Benchmark numbers in the doc are not reproducible; the table overstates the gap

**File:** `docs/decisions/tsc-alignment.md:22-25`.

The doc reports `986 ns` (offset) vs `1591 ns` (MSR). I could not reproduce the `1591`. On the same box,
2026-06-10, `release`, 5 runs of 10,000 calls each:

```
offset-attr ≈ 0.91–0.99 µs/call   (matches the doc's 986)
msr-write   ≈ 1.05–1.17 µs/call   (NOT 1591)
```

Two things explain the discrepancy and both are worth fixing in the doc:

1. **The in-tree test that produced the numbers cannot run in release** — it panics on the
   `get_tsc_offset` round-trip (the Critical). So the committed `986/1591` figures were produced in
   **debug**, where absolute ns are meaningless for a perf claim and the MSR path's debug overhead
   inflates its number. After the Critical is fixed, re-run in `--release` and update the table.
2. **Allocation is not the gap.** I tested the hypothesis that `set_tsc_value_msr`'s per-call
   `Msrs::from_entries` allocation unfairly taxes the MSR path. Hoisting the `Msrs` and mutating
   `as_mut_slice()[0].data` in place changed nothing (alloc ≈ hoist ≈ 1.10 µs). The cost is the ioctl +
   KVM's TSC-write fast path, not the allocation. So the comparison is already fair on that axis — but
   the honest gap is ≈ **+0.15 µs** (≈15–18%), not the ≈ +0.6 µs the table implies.

**The decision is unaffected** — offset-attr is faster *and* avoids the heuristic hazard, which is the
real reason to choose it. But replace the table with release numbers (and ideally min-of-N or median, not
mean, given ioctl jitter) so the doc rests on honest, reproducible figures. The derived "4.8 ms/guest-s"
line should be recomputed from the corrected MSR number (with `1100 ns × 3000 ≈ 3.3 ms/guest-s`).

## S2 — `set_tsc_value_msr`'s `let _ = &mut msrs;` is a confused no-op; drop it

**File:** `crates/dh-vmm/src/tsc.rs:105`.

```rust
let n = vcpu.set_msrs(&msrs)...;   // shared borrow, the actual ioctl
let _ = &mut msrs;                 // <- no-op AFTER the call
```

This line takes a throwaway `&mut` to `msrs` *after* `set_msrs` already consumed it via a shared borrow.
It does nothing (`msrs` is dropped on the next line) and reads as a half-remembered attempt to defeat
optimization. It is harmless here because `set_msrs` is a real read-only ioctl on a heap-backed FAM
buffer (the data genuinely travels to the kernel), but it is dead code that will confuse the next reader —
especially adjacent to the GET path that has a *real* optimization-soundness problem. Delete it.

## S3 — `has_tsc_offset_attr` swallows the errno; distinguish "absent" from "fd error"

**File:** `crates/dh-vmm/src/tsc.rs:48-55`.

`has_tsc_offset_attr` returns `rc == 0`, collapsing every non-zero return to "not supported". The kernel
distinguishes `ENXIO`/`ENOENT` (attr genuinely absent — the answer we want) from `EBADF`/`EFAULT`
(programming/fd error). Today the live test `assert!`s the attribute exists, so a regression that made
this fail would surface — but the function is public and named like a clean predicate. Consider returning
`Result<bool, KvmError>` (or at least `errno == ENXIO || ENOENT => false`, else propagate) so an M4 caller
can tell "this kernel lacks the feature, fall back" from "we passed garbage". Minor; current behavior is
acceptable given the asserting test, but the cleaner shape costs little.
