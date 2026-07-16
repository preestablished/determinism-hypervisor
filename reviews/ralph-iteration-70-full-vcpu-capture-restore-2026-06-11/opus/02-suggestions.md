# Suggestions (non-blocking)

## S1 — Stale binding-version reference in a SAFETY comment

`vcpu_state.rs:185–186`:

```rust
// SAFETY: plain kvm_xsave (no FAM tail in the 0.14 binding); the area
// is the canonical form whose clear bits XRSTOR treats as init.
```

The crate actually pins **kvm-bindings 0.13.0** (verified at
`~/.cargo/registry/.../kvm-bindings-0.13.0/`). The comment says "0.14 binding".
The technical claim is still correct for 0.13 (`kvm_xsave` is `region: [u32;
1024]` plus an `__IncompleteArrayField<u32> extra` FAM tail — the code only
writes `region[..]`, leaving the tail empty, which is exactly the fixed-size
`KVM_SET_XSAVE` semantics). But the version number should match reality to avoid
future-reader confusion.

Suggested fix:

```rust
// SAFETY: plain kvm_xsave (we write only the fixed `region` array; the
// __IncompleteArrayField tail in the 0.13 binding stays empty, matching
// the fixed-size KVM_SET_XSAVE ioctl); the area is the canonical form
// whose clear bits XRSTOR treats as init.
```

## S2 — Decode does not assert struct sizes match the wire contract

`decode_section` relies on `size_of::<T>()` for each struct's wire width, which
is correct *for a given kvm-bindings version* but silently re-frames the whole
section if a future bindings bump changes a struct's size (e.g. a new reserved
field). The MSR list is explicitly code-versioned and the section carries
`VCPU_SECTION_VERSION`, so a deliberate bump is the intended escape hatch — but
an *accidental* size drift from a dependency update would pass `VERSION == 1` and
produce a structurally-valid-but-wrong decode on a peer running a different
bindings minor.

Consider a `const _: () = assert!(size_of::<kvm_regs>() == 144 && …)` block (or a
single test) pinning the expected sizes, so a bindings bump that changes a
captured struct's ABI fails the build loudly and forces a `VCPU_SECTION_VERSION`
decision. Low effort, catches a silent cross-host divergence class.

## S3 — `as i64` / `as u64` casts in the TSC path could be annotated

`vcpu_state.rs:208`: `vns.wrapping_sub(host_tsc) as i64`. The `wrapping_sub`
correctly produces the two's-complement offset and the `as i64` reinterprets the
bit pattern — this is exactly right for a signed TSC offset. A one-line comment
("two's-complement reinterpret: KVM treats the attr payload as i64 offset")
would save the next reader from re-deriving that the wrap + cast is intentional
rather than a sloppy narrowing. Purely cosmetic.

## S4 — Live round-trip test could perturb a restorable MSR / xcrs field

`live_get_set_get_roundtrip` perturbs only `regs.rax`/`regs.rip` before the
capture. The GET→SET→GET fixed-point assertion is strong, but the *perturbation*
exercises only the REGS path; a divergence introduced in (say) the XCRS or
DEBUGREGS byte-copy would still be caught by the fixed-point equality (because
both captures would carry the same garbage) only if KVM round-trips that field
losslessly — which is the very property under test. Perturbing one
DEBUGREGS/XCRS-reachable field too (e.g. `dr7` via `set_debug_regs`) would widen
the live coverage at near-zero cost. Optional; the synthetic-state codec test
already covers all fields byte-exactly.
