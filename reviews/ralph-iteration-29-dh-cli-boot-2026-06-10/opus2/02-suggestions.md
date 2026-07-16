# Suggestions

### S-1 — MSR filter is never applied in the M0 path; document why no MSR exits occur (flag for s0p)

**Files:** `tools/dh-cli/src/boot.rs` (`boot()` never calls `dh_vmm::msr::apply_default_deny_filter`);
cross-ref `crates/dh-vmm/src/msr.rs`, `crates/dh-vmm/src/kvm.rs` (`create_slot_vm`).

`create_slot_vm` enables `KVM_CAP_X86_USER_SPACE_MSR` with `KVM_MSR_EXIT_REASON_FILTER`, but the M0
boot path **never calls `KVM_X86_SET_MSR_FILTER`** (no `apply_default_deny_filter`). I confirmed
against `msr.rs`: `USER_SPACE_MSR + EXIT_REASON_FILTER` only routes *filter-denied* MSR accesses to
userspace. **With no filter installed, nothing is denied, so KVM handles every MSR in-kernel and no
`X86Rdmsr/X86Wrmsr` exit can occur.** This is correct and fine for M0 — the nanokernel guests
(`hello`, `pipeline_smoke`, `landing_loop`) issue **no** RDMSR/WRMSR (verified in the asm). If a
future guest *did* WRMSR in this path, KVM would execute it in-kernel (potential nondeterminism /
host-state leak), exactly the R6 case the filter prevents.

**Suggestion:** Add one line to the `boot.rs` module header: "M0 installs **no** MSR filter, so
KVM handles all MSRs in-kernel and no MSR exits occur; the nanokernel guests issue no MSR ops. The
s0p boot path MUST call `apply_default_deny_filter` after `create_slot_vm` before running any guest
that could touch MSRs." This makes the omission a documented M0 boundary, not a silent gap.

### S-2 — `enter_long_mode` segment/CR setup is correct but fragile; pin the assumptions in a comment

**File:** `tools/dh-cli/src/boot.rs` — `enter_long_mode` (lines ~215–259).

It runs live, but several values rely on undocumented KVM/VMX leniency:
- **`efer = LME | LMA` set by hand** with `cr0.PG=1` consistent — good, but some KVM versions are
  strict about LMA being set only when paging is active; this happens to be consistent here. Worth a
  note that LMA is set deliberately (KVM does not derive it for you in SET_SREGS).
- **`cr0 = 0x8000_0021` = PG | NE | PE.** The `ET` bit (bit 4) is omitted; on all modern CPUs ET is
  hard-wired to 1 and KVM does not care, but a one-word comment ("ET is always 1; we don't set it")
  removes the head-scratch.
- **`TR` is left at the `get_sregs` default.** VMX entry on Intel requires a usable TR (the
  processor checks TR.type). It worked because a freshly-created vCPU's `get_sregs` returns a valid
  busy-TSS TR. This is the most fragile assumption in the function — if KVM ever changed the vCPU
  reset TR, entry would `KVM_RUN`-fail with a cryptic emulation error. Add a comment: "relying on
  the vCPU-reset TR being a valid 16-bit busy TSS — VMX requires a usable TR for entry; we never
  touch it."
- **`ss`/`ds` with `db=1` in 64-bit mode** — D/B is ignored for data segments in long mode, fine;
  the explicit `db: 1` on the data descriptor (vs `db: 0`, `l: 0` derived from `code`) is harmless
  but a note ("db/l ignored for 64-bit data segments") would help.
- **`rflags = 2`** (only the reserved bit-1) — correct minimal value.

None of these are bugs; they're load-bearing assumptions that deserve to be pinned so a future KVM
or CPU change fails *loudly with context* rather than mysteriously.

### S-3 — Add a live-gated landing_loop determinism test in dh-cli

There is currently no determinism test for `landing_loop` in `tools/dh-cli/` (only the M0 hello /
pipeline_smoke acceptance). I verified determinism by hand (same cmdline → identical
`{serial, exits}` across runs; distinct cmdlines all reach `L`). Capture it as a test, gated on
`kvm_usable()` like the existing ones:

```rust
#[test]
fn landing_loop_is_deterministic_across_runs() {
    if !kvm_usable() { eprintln!("skipping: /dev/kvm not usable"); return; }
    let a = boot(nanokernel::landing_loop_elf(), 16 << 20, b"100", 10_000).unwrap();
    let b = boot(nanokernel::landing_loop_elf(), 16 << 20, b"100", 10_000).unwrap();
    assert_eq!((&a.serial, a.exits), (&b.serial, b.exits));
    assert_eq!(a.serial, b"L");
}
```

This is the determinism guarantee the whole project exists to protect; an M0 regression test on the
long-runner is cheap and high-value.

### S-4 — Validate / clamp `--mem-mib` against `<< 20` overflow; give a clean error

**File:** `tools/dh-cli/src/main.rs` — `boot_cmd`, `mem_mib << 20`.

`--mem-mib` parses to `u64` then shifts left 20. A value ≥ 2^44 overflows the `u64` and silently
wraps (I confirmed: `--mem-mib 17592186044416` wraps to `0` bytes → cryptic
`Invalid argument (os error 22)` from the memfd). Similarly `--mem-mib 0` yields the same opaque
EINVAL. Neither is a security issue (trusted-operator debug CLI), but both are bad UX. Suggest a
small guard before the shift:

```rust
let mem_bytes = mem_mib.checked_shl(20)
    .filter(|&b| (1..=1 << 30).contains(&b))
    .unwrap_or_else(|| { eprintln!("dh-cli boot: --mem-mib must be 1..=1024"); std::process::exit(2) });
```

This also makes the 1-GiB cap an *argument* validation (early, clear) rather than surfacing as the
`boot()` Mem error later — both are fine, but front-loading it reads better.

### S-5 — `load_elf` does not reject overlapping / low PT_LOAD segments that would clobber the page tables or BootInfo

**File:** `tools/dh-cli/src/boot.rs` — `load_elf` (lines ~130–173).

The loader copies each PT_LOAD to its `p_vaddr` with no check that the target range avoids the
loader's own structures at `0x1000` (PML4), `0x2000` (PDPT), `0x3000` (PD), `0x5000` (BootInfo). The
nanokernel link script loads at `0x100000`, well clear, so this is safe for *these* guests — but a
hostile or mis-linked ELF with a PT_LOAD at, say, `0x1000` would silently overwrite the page tables
*after* `load_elf` runs but the writes happen in `boot()` order (load_elf → page_tables → bootinfo),
so page tables would actually win and the guest segment would be partially clobbered instead —
non-obvious either way. For M0 with trusted test ELFs this is acceptable; a one-line guard
(`if p_vaddr < 0x10_0000 { return Err(bad("PT_LOAD overlaps loader low memory")); }`) would make the
loader robust against drift and document the reserved low region. Low priority.
