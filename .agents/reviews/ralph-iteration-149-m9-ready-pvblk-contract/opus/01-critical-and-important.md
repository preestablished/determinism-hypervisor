# Critical And Important Findings

## Critical

1. `crates/dh-vmm/src/boot/linux_bzimage.rs:292`, `crates/dh-vmm/src/boot/linux_bzimage.rs:576`, `crates/dh-vmm/src/boot.rs:241`

Problem: The loader derives `kernel_payload_file_offset` as `setup_bytes + payload_offset`, copies only `payload_length` bytes to `LINUX_KERNEL_LOAD_GPA`, then enters at `LINUX_KERNEL_LOAD_GPA + 0x200`. In the Linux x86 boot protocol, `payload_offset` is the offset from the beginning of the protected-mode code to the compressed payload; it is not the start of the loaded kernel image. The loaded kernel image starts at `(setup_sects + 1) * 512` in the bzImage file, and the `+0x200` 64-bit entry is relative to that loaded image. With a real bzImage this skips the startup/decompressor bytes and points RIP into the wrong data.

Suggested fix: Treat `setup_bytes` as the protected-mode kernel file offset. Copy the protected-mode kernel image from `bzimage[setup_bytes..]` or the `syssize`-bounded equivalent to `LINUX_KERNEL_LOAD_GPA`; keep `payload_offset/payload_length` only for validation/metadata. Update `kernel_payload` naming/tests accordingly.

Reference: Linux x86 boot protocol, "LOADING THE REST OF THE KERNEL" and "64-bit BOOT PROTOCOL": https://www.kernel.org/doc/Documentation/x86/boot.txt

## Important

1. `crates/dh-vmm/src/boot.rs:397`, `crates/dh-vmm/src/boot/linux_bzimage.rs:760`

Problem: The synthetic bzImage fixtures encode the same wrong model as the implementation: meaningful bytes only exist at `setup_bytes + payload_offset`, and the tests assert those bytes appear at `LINUX_KERNEL_LOAD_GPA`. That locks in the protocol bug instead of catching it.

Suggested fix: Build synthetic images with distinct nonzero bytes in the protected-mode startup area before `payload_offset`, then assert the full protected-mode image is copied and that byte `LINUX_KERNEL_LOAD_GPA + 0x200` comes from the startup image, not from the compressed payload.

2. `tests/determinism/tests/linux_boot_trace.rs:35`

Problem: The ignored Linux smoke test only calls `load_bzimage_and_enter` and checks register/boot_params state. It never executes `KVM_RUN`, so it would not catch the bad entry bytes above, unsupported early MSRs, APIC assumptions, or immediate triple-fault behavior.

Suggested fix: When artifacts are present, run a bounded first-exit trace and assert a classified early exit or successful deterministic milestone. Keep the pre-entry register assertions, but do not let them be the whole Linux boot smoke.
