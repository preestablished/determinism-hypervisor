# Critical and Important Findings

## CRITICAL — `get_tsc_offset` reads kernel output back through a shared, non-`mut` local: UB + miscompilation hazard

**File:** `crates/dh-vmm/src/tsc.rs:77-90` (and `attr_for`, lines 39-46)

```rust
pub fn get_tsc_offset(vcpu: &VcpuFd) -> Result<i64, KvmError> {
    let raw = 0u64;                       // (1) NOT `mut`, no UnsafeCell
    let attr = attr_for(Some(&raw));      // (2) &u64 -> *const u64 -> u64 in attr.addr
    let rc = unsafe { ioctl_with_ref(&vcpu.as_raw_fd(), KVM_GET_DEVICE_ATTR(), &attr) };
    // ... kernel writes 8 bytes to the address stored in attr.addr ...
    Ok(raw as i64)                        // (3) reads `raw` back
}
```

The kernel `KVM_GET_DEVICE_ATTR(TSC_OFFSET)` handler **writes** the offset into the memory
pointed to by `attr.addr`. That memory is the local `raw`, whose address was taken as a
**shared reference** (`&raw`) on a binding that is **not declared `mut`** and is **not wrapped
in `UnsafeCell`**. This is unsound on two independent grounds:

1. **Mutation behind a shared reference without interior mutability is UB.** Under Stacked
   Borrows / Tree Borrows, the pointer derived from `&raw` carries a read-only
   (`SharedReadOnly`) tag. Writing through it — even from the kernel, which from Rust's
   abstract-machine perspective is still "a write to memory Rust believes is immutable for
   the duration of the borrow" — violates the aliasing model. The `unsafe` block does not
   license this; `unsafe` suspends the borrow *checker*, not the *aliasing rules* the
   optimizer relies on.

2. **`raw` is not `mut`, so the read-back is optimizer-foldable.** Nothing in the visible
   Rust program ever assigns to `raw` after its `0u64` initialization. The compiler is
   therefore entitled to treat `raw as i64` at line 89 as the constant `0`, propagate it,
   and never reload from the stack slot. In a release / LTO build this can make
   `get_tsc_offset` **always return 0** regardless of what the kernel wrote — and the live
   round-trip assert (`assert_eq!(get_tsc_offset(...).unwrap(), -123_456_789)`) would then
   *fail* in release, or worse, silently misreport offsets in production diagnostics. The
   test passes today only because it runs in a **debug build** where this optimization is
   not applied. **A passing debug test is not proof of soundness here.**

This is not theoretical pedantry: ground (2) is a concrete miscompilation the optimizer is
permitted to perform, and the function's entire purpose (verification / diagnostics) is
defeated by it.

### Why `set_tsc_offset` is fine and only `get` is broken

In `set_tsc_offset` (lines 60-74) the kernel only **reads** through `attr.addr`. Passing
`&raw` (shared, immutable) is exactly correct there — same pattern as msr.rs/inject.rs,
which also pass `&T` for kernel-read structs. The bug is specific to the **GET** direction
where the kernel writes.

### Fix

Make the backing local mutable and derive the address from a `&mut` (a `*mut`):

```rust
pub fn get_tsc_offset(vcpu: &VcpuFd) -> Result<i64, KvmError> {
    let mut raw = 0u64;
    let attr = attr_for_mut(&mut raw);   // addr = &mut raw as *mut u64 as u64
    let rc = unsafe { ioctl_with_ref(&vcpu.as_raw_fd(), KVM_GET_DEVICE_ATTR(), &attr) };
    if rc != 0 { /* ...unchanged... */ }
    Ok(raw as i64)
}
```

Note the *outer* `ioctl_with_ref(&attr)` stays correct — the kernel reads the `kvm_device_attr`
struct itself (so `&T` / `*const` is right for `attr`). The unsoundness is entirely in how
the **inner** `raw`'s address is obtained. `attr_for` should expose a mut path for GET. A
minimal shape that keeps both call sites honest:

```rust
fn attr_for(offset_ptr: u64) -> kvm_device_attr {
    kvm_device_attr { flags: 0, group: KVM_VCPU_TSC_CTRL,
                      attr: u64::from(KVM_VCPU_TSC_OFFSET), addr: offset_ptr }
}
// SET / HAS:  attr_for(core::ptr::addr_of!(raw) as u64)   // or 0 for HAS
// GET:        attr_for(core::ptr::addr_of_mut!(raw) as u64)  // &mut-derived
```

Using `core::ptr::addr_of_mut!(raw)` (or `&mut raw as *mut u64`) gives the pointer write
provenance and forces `raw` to be a real, observably-mutated stack slot. The 0.15.0
`vmm-sys-util` you have locked also exposes `ioctl_with_mut_ptr` / `ioctl_with_mut_ref` if
you prefer to thread mutability through the ioctl wrapper as well, but for the *attr* struct
that is unnecessary — only the inner data pointer matters.

After the fix, re-run the live round-trip in **release** too (`cargo test -p dh-vmm tsc
--release -- --nocapture`) to demonstrate the fold hazard is gone.

---

## IMPORTANT — `attr_for` launders a `&u64` to an integer, hiding the read-only provenance from reviewers and tooling

**File:** `crates/dh-vmm/src/tsc.rs:39-46`

```rust
addr: offset_ptr.map_or(0, |p| p as *const u64 as u64),
```

Independent of the Critical fix, `attr_for` takes `Option<&u64>` (a *shared* ref) for **all
three** callers, including the GET path that needs write provenance. The `as *const u64 as
u64` int-cast also erases provenance from the borrow model's view at the cast point — which
is *why* the bug is easy to miss and why the SAFETY comment on line 80 ("the kernel writes
the u64 through `addr`") is contradicted by the `&u64` it was handed.

Even after fixing GET, the shared signature is a latent trap: the next person who adds a
write-direction attr will reach for the same helper and reintroduce the UB. Split the helper
so the **type** encodes direction:

- `attr_read(&u64)` / `attr_none()` for SET and HAS (kernel reads or ignores `addr`)
- `attr_write(&mut u64)` for GET (kernel writes `addr`)

This makes the borrow model and the reader agree, and makes the SAFETY comments true by
construction rather than by hand-waving. (If you keep a single helper taking a `u64` address
as above, the discipline moves to the call sites — acceptable, but document that GET must
pass an `addr_of_mut!`-derived address.)
