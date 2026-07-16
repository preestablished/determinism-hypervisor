# Positive Notes

1. **The `_IOW`-not-`_IOWR` subtlety is correctly handled and documented.** All three ioctl
   numbers match `/usr/include/linux/kvm.h:1519-1521` exactly — `KVM_SET_DEVICE_ATTR` 0xe1,
   `KVM_GET_DEVICE_ATTR` 0xe2, `KVM_HAS_DEVICE_ATTR` 0xe3, **all `_IOW`**, including GET. The
   inline comment (tsc.rs:34-36) and the decision doc's implementation note both explain
   *why* GET is `_IOW` (the kernel writes through `attr.addr`, but the `kvm_device_attr`
   struct itself only travels userspace→kernel, so the direction encoded in the request is
   "write the struct in"). This is exactly the kind of kernel-uapi gotcha that an EINVAL
   would otherwise eat hours on, and it is captured for the next reader. (The irony is that
   the *correct* reasoning recorded here — "the kernel writes through `addr`" — is precisely
   what the GET implementation then fails to honor on the Rust side; see Critical.)

2. **The raw-ioctl rationale is sound and justified.** `kvm-ioctls` 0.24 cfg-gates its
   device-attr wrappers to aarch64 (an upstream gap; the ioctls are valid on x86), so
   issuing them raw in the msr.rs style is the right call, and the module says so.

3. **The benchmark actually answers the bead's question and the decision is recorded before
   M4 freezes restore** — which is the entire point of bead 3np ("PICK ONE, recorded in
   docs, BEFORE M4 freezes restore behavior"). The decision (offset attr, one write per
   restore; MSR path reference-only with a not-wired-into-restore guard) is unambiguous and
   matches ARCH §4 defense-4's stated preference for offset writes over MSR value writes.

4. **The MSR sync-heuristic hazard is correctly identified as the *real* reason** to avoid
   the per-entry path — not just the ~0.5% ioctl tax. The doc is careful to say the offset
   attr round-trips **bit-exactly** while the MSR write is rebased onto the running host TSC
   (no readable exactness guarantee). That framing is the determinism-relevant one.

5. **The `i64 → u64 as`-cast round-trip is correct**, and the test deliberately exercises a
   **negative** offset (−123,456,789 — guest behind host), which is the meaningful direction
   for restore (`offset = vns − host_tsc_at_resume` is typically negative early in a run).
   Good test-value choice.

6. **Clean integration:** `pub mod tsc;` added to lib.rs, no churn elsewhere; clippy clean,
   fmt clean, full workspace green; the test self-skips when `/dev/kvm` is unusable
   (`kvm_usable()` guard) so it does not break non-KVM CI. The `set_msrs` `n != 1` guard is a
   reasonable defensive check on the ioctl's reported count.
