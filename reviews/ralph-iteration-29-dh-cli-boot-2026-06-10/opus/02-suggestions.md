# Suggestions

These are non-blocking. Several are about documenting fragility that is correct *today*
but kernel-version- or future-guest-sensitive.

### S-1 — Document the LMA-pre-set / CR0 long-mode fragility

`enter_long_mode` sets `cr0 = 0x8000_0021` (PG | NE | PE) and
`efer = (1<<8) | (1<<10)` (LME | LMA) in one `KVM_SET_SREGS`, with PG already on. This
works on the lab box, but it sidesteps the architectural long-mode-activation sequence
(set LME, then enable paging, and the CPU sets LMA itself). KVM's `KVM_SET_SREGS`
path generally accepts a pre-set LMA as long as the (EFER.LMA == (EFER.LME && CR0.PG))
consistency invariant holds — which it does here — so it is valid. Two things worth
noting in a comment so a future kernel bump doesn't silently break it:

- **Missing CR0.MP (bit 1) and CR0.ET (bit 4).** `0x21` = PE|NE only. MP/ET are
  effectively don't-cares for a guest that never touches the x87/SSE task-switch path,
  and KVM does not reject their absence — but the moment a guest executes an FPU/SSE
  instruction with `CR0.MP=0` / `CR0.EM` semantics in play, behavior diverges from a
  "normal" boot. For the nanokernel stubs (no FPU) this is fine; flag it as an M0-only
  shortcut so the real loader (bead s0p) sets a complete CR0.
- **No `KVM_SET_SREGS` consistency-check failure handling beyond the generic
  `kvm_err`.** If a future kernel tightens the LMA/LME/PG cross-check, this surfaces as
  a `KVM_SET_SREGS` EINVAL with a bare message. A one-line comment naming the invariant
  being relied on would make that failure self-diagnosing.

Suggested: a short comment block above the CR0/EFER writes stating
"M0 relies on KVM accepting LMA pre-set with PG=1 (LMA==LME&&PG holds); CR0 omits MP/ET
intentionally — no-FPU guests only; bead s0p sets a complete CR0."

### S-2 — Segment caches without a backing GDT: name the unusable TR/LDT assumption

The code sets CS/DS/ES/FS/GS/SS cached descriptors directly and leaves
`gdt`, `idt`, `tr`, `ldt` at the `get_sregs()` defaults. KVM's reset defaults leave
`tr`/`ldt` in the usable-but-null state it provides, and the guest never reloads a
segment (no `mov %ax,%ds`, no far jump, no task switch), so the stale `gdt.base/limit`
are never consulted. This is correct for a guest that performs zero segment reloads —
which the nanokernel stubs guarantee. Worth a comment: "valid only because the guest
never reloads a segment register or uses TR/LDT; any guest that does needs a real GDT
in memory." This pairs with S-1 as the set of M0-shortcut invariants the real loader
must close.

### S-3 — No IDT → any exception triple-faults to Shutdown: assert the posture explicitly

With no IDT installed, any guest exception (page fault, #UD, #GP, divide error)
escalates to a triple fault → `KVM_EXIT_SHUTDOWN`, which `run_until_hlt` maps to
`ExitEvent::Shutdown` and returns as `UnexpectedExit("Shutdown")`. That is the right
M0 posture (fail loudly, no silent corruption), but it is implicit. Suggest:
(a) a comment stating "no IDT is intentional — any guest fault triple-faults to
Shutdown, surfaced as an error; M0 guests must not fault"; and (b) consider a dedicated
`BootError` variant or message for Shutdown so a triple-faulting guest reports
"guest faulted (triple fault / Shutdown)" rather than the generic `UnexpectedExit`,
which reads as a harness bug rather than a guest bug.

### S-4 — `DetcallIn` reads zero, not the IDENT magic: document the M0 divergence

In `run_until_hlt`, every `VcpuExit::IoIn` is `data.fill(0)` before classification, so a
detcall IDENT read returns `0x0000_0000` instead of `0xD37E_0001`. For M0 this is moot
(no detchannel host exists, and the device-exercise guest would hit MMIO and error out
first), but it is a latent surprise for anyone who points a detcall-using guest at this
loop. A one-line comment in the `IoIn` arm — "M0 fills ALL INs (serial + detcall) with
zeros; detcall IDENT therefore reads 0x0, not 0xD37E0001 — fine because no M0 guest
issues a detcall IN" — would prevent a confusing future debugging session.

### S-5 — Add a `landing_loop` live test (fast, high-value coverage)

The brief notes landing_loop runs 100M instructions in ~40ms / 2 exits. That is cheap
and exercises a genuinely different path from hello (long compute, BootInfo cmdline
parsing, ring-buffer touches across multiple 2 MiB pages). Suggest adding a third
kvm-gated test that boots `landing_loop_elf()` with a small cmdline iteration count and
asserts the terminal `L` serial byte. This guards page-table coverage of multi-page RAM
and cmdline plumbing — neither of which hello/pipeline_smoke exercise — for near-zero
runtime.

### S-6 — Hoist `kvm-bindings` / `kvm-ioctls` / `vm-memory` to `[workspace.dependencies]`

`tools/dh-cli/Cargo.toml` pins `kvm-bindings = "0.14.0"`, `kvm-ioctls = "0.24.0"`,
`vm-memory = "0.18.0"` — the same crates dh-vmm depends on. Since dh-cli links dh-vmm
and passes `vm_memory::GuestMemoryError` / `kvm_ioctls::VcpuExit` types across the
boundary, a version skew between the two manifests would be an ABI mismatch caught only
at compile time (or worse, a duplicate-crate link error). Promote these three to
`[workspace.dependencies]` and reference them as `.workspace = true` (the manifest
already does this for `dh-vmm`, `license`, etc.), eliminating the drift surface.

### S-7 — `--mem-mib` overflow and zero are silently odd

`mem_mib << 20` with `mem_mib: u64` is fine for sane inputs, but `--mem-mib 0` boots a
0-byte VM (likely an opaque KVM/memfd error downstream), and very large values that
exceed the 1 GiB cap produce the clean `BootError::Mem` only after KVM open + slot
create work. Minor: reject `mem_mib == 0` and values `> 1024` at arg-parse time with the
usage message, so the failure is immediate and self-explanatory rather than a KVM error
or a deferred cap rejection. Also `mem_mib << 20` for `mem_mib > 16_777_215` (16 Ti)
would overflow `u64`, but the 1 GiB cap makes that unreachable in practice — the
arg-time bound closes it cleanly.
