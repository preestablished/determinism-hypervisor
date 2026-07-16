# Action items

Self-contained items distilled from findings 01–02. None block merge; all are M0-appropriate
documentation/robustness fixes or follow-on beads.

### Critical

_None._

### Important

- [ ] **(I-1) Correct the "MMIO error" claim in `tools/dh-cli/src/boot.rs`.** The module header
  and the `MmioRead`/`MmioWrite` arms in `run_until_hlt` imply a device touch surfaces
  `UnexpectedExit("MMIO at {gpa} ...")`. It does **not**: the M0 page tables identity-map only
  `[0, mem_bytes)` (≤ 1 GiB), so the MMIO hole at `0xD000_0000` is never present in guest paging,
  and a device access page-faults → triple fault → `VcpuExit::Shutdown` *before* any MMIO exit.
  Live-verified: `device_exercise` returns `unexpected exit: Shutdown` at both 16 MiB and 1 GiB RAM.
  Fix: annotate the two MMIO arms as "unreachable in M0 (the identity map never covers the hole);
  retained for the s0p loader which maps the hole as a no-memslot region," and update the header
  comment. Optionally enrich the `Shutdown` message with a "(triple fault — guest touched a device
  window or unmapped RAM?)" hint, since today a device touch prints only `unexpected exit: Shutdown`.

- [ ] **(I-2) Comment or fix the unconditional `IoIn` RAZ-fill in `run_until_hlt`.** The
  `VcpuExit::IoIn(_port, data) => data.fill(0)` arm catches **every** IN port, bypassing
  `classify_exit` and its `DetcallIn`/`SerialIn` IN-FILL contract entirely. Safe for current guests
  (`hello`/`landing_loop` use blind `out`, never poll), but a future 16550 driver polling the LSR at
  `0x3FD` would read `0x00` forever → spin. Either route INs through `classify_exit`, or add a guard
  comment: "M0 RAZ-fills all IN ports including serial LSR `0x3FD`; a status-polling 16550 driver
  would hang — the serial-device bead (avm) must model LSR before any guest polls." The
  `SERIAL_BASE..SERIAL_END` constants already exist (used by the OUT arm) and could gate the IN arm.

### Suggestions

- [ ] **(S-1) Document the absent MSR filter (flag for s0p).** M0 never calls
  `apply_default_deny_filter`, so with an empty filter KVM handles all MSRs in-kernel and **no** MSR
  exits occur — correct for the no-MSR nanokernel guests. Add a header note that s0p MUST install the
  filter after `create_slot_vm` before running any MSR-touching guest (else R6 host-state leak).

- [ ] **(S-2) Pin the fragile long-mode-entry assumptions in `enter_long_mode` comments:** LMA is set
  by hand (KVM does not derive it); `cr0` omits the always-1 `ET` bit; **TR is left at the vCPU-reset
  default and VMX entry requires a usable TR** (the single most fragile assumption); `db`/`l` are
  ignored for 64-bit data segments. None are bugs; pinning them makes a future KVM/CPU change fail
  loudly with context.

- [ ] **(S-3) Add a live-gated `landing_loop` determinism test** in `tools/dh-cli/tests/` (skip on
  `!kvm_usable()`): boot twice with the same cmdline, assert identical `{serial, exits}` and
  `serial == b"L"`. Captures the project's core guarantee as a regression test.

- [ ] **(S-4) Guard `--mem-mib` in `main.rs`:** `mem_mib << 20` overflows/wraps for values ≥ 2^44
  (verified: `--mem-mib 17592186044416` → 0 bytes → cryptic EINVAL), and `--mem-mib 0` gives the same
  opaque error. Use `checked_shl(20)` + a `1..=1<<30` byte range check with a clear "must be 1..=1024"
  message.

- [ ] **(S-5) (low) Reject low/overlapping PT_LOAD in `load_elf`:** a PT_LOAD with `p_vaddr <
  0x10_0000` would target the loader's reserved low region (page tables `0x1000–0x3000`, BootInfo
  `0x5000`). Safe for the current link-script guests (load @ `0x100000`); a one-line guard documents
  the reserved region and hardens against drift.
