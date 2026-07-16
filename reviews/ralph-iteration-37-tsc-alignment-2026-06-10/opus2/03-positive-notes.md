# Positive notes

## P1 — The decision is correct and rests on the right reason

Choosing `KVM_VCPU_TSC_OFFSET` over per-entry `KVM_SET_MSRS{IA32_TSC}` is the right call, and the doc
leads with the *durable* reason: the offset attribute is heuristic-free and survives entries, whereas a
value write can be silently quantized onto an existing TSC-sync generation (a real determinism hazard).
That argument stands independent of the benchmark, which is the correct way to anchor a determinism
decision — perf is the tiebreaker, not the basis.

## P2 — The `_IOW` direction of `KVM_GET_DEVICE_ATTR` was correctly diagnosed

The non-obvious fact that `KVM_GET_DEVICE_ATTR` is `_IOW` (not `_IOWR`) — the kernel writes through
`attr.addr`, not the struct — is correctly identified in both the code comment (tsc.rs:34-35) and the doc
(:42-44). I confirmed against `/usr/include/linux/kvm.h:1520`: `KVM_GET_DEVICE_ATTR _IOW(KVMIO, 0xe2,
struct kvm_device_attr)`. The `ioctl_iow_nr!` numbers (`0xAE, 0xe1/0xe2/0xe3`) all match the uapi. (The
irony is that getting this *fact* right is what makes the Critical UB so close to correct — the design is
right; only the Rust-side aliasing of the write target is wrong.)

## P3 — The raw-ioctl approach is genuinely justified, not a shortcut

I verified the doc's claim against the vendored crate. kvm-ioctls 0.24 `src/ioctls/vcpu.rs` gates
`set_device_attr` (line 278) and `has_device_attr` (line 318) behind `#[cfg(target_arch = "aarch64")]`,
and ships **no** `get_device_attr` on any arch. So on x86_64 there is no upstream wrapper to call; issuing
the ioctls raw, msr.rs-style, is the correct workaround, and the comment accurately describes the upstream
gap.

## P4 — The live test is the right shape for the kvm-intel lane

Asserting `has_tsc_offset_attr` (rather than skipping) means the kvm-intel CI runner — same kernel family
(6.8, `KVM_CAP_VCPU_ATTRIBUTES = 127` present) — hard-fails if the attribute ever disappears, instead of
silently going green on a skipped leg. The exact bit-for-bit round-trip assertion is exactly what you want
for a value the restore path will depend on. (It only needs to also run in release — see the Critical.)

## P5 — Clean separation of "chosen" vs "reference" mechanisms

`set_tsc_value_msr` is clearly documented as a benchmarked reference that "must not be wired into restore"
(doc :38-40, code :92-94), and the chosen `set_tsc_offset` is annotated as the M4 mechanism. Keeping the
loser around as a measurable, labeled reference (rather than deleting it) is good engineering hygiene for a
decision record — future readers can re-run the comparison.
