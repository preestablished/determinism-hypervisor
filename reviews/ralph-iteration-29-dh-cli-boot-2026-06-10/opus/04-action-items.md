# Action items

Self-contained list, prioritized. None block merge; I-1 should be fixed before any
consumer machine-parses `--json` output.

### Critical

None.

### Important

- [ ] **Fix `--json` invalid-JSON escaping** (`tools/dh-cli/src/main.rs`, `boot_cmd`
      JSON branch). `std::ascii::escape_default` emits `\xNN` for non-printable bytes,
      which is not a legal JSON escape — output is malformed JSON the moment a guest
      writes a control byte. Replace with a JSON-correct escaper that emits `\uXXXX` for
      control/non-printable bytes and the named short escapes (`\n \r \t \" \\`) where
      applicable, passing printable ASCII (`0x20..=0x7e`) through. `HELLO\n` escapes
      cleanly today, which is why the live acceptance never caught it. See 01 §I-1 for a
      drop-in function. Add a regression test that `--json`-boots a guest emitting a
      control byte and parses the result as JSON.

### Suggestions

- [ ] **Document the long-mode-entry shortcuts** (`boot.rs::enter_long_mode`): comment
      that M0 relies on KVM accepting EFER.LMA pre-set with CR0.PG=1 (the
      `LMA == LME && PG` invariant holds), and that CR0 intentionally omits MP/ET
      (`0x21` = PE|NE only) — valid only for no-FPU guests; bead s0p sets a complete
      CR0. (02 §S-1)
- [ ] **Document the no-GDT / unusable-TR-LDT assumption** (`enter_long_mode`): the
      cached segment descriptors are valid only because the guest never reloads a
      segment register or uses TR/LDT. (02 §S-2)
- [ ] **Make the no-IDT / triple-fault posture explicit** (`run_until_hlt`): comment
      that any guest fault triple-faults to `KVM_EXIT_SHUTDOWN` by design, and consider
      a dedicated error message ("guest faulted (triple fault / Shutdown)") instead of
      the generic `UnexpectedExit("Shutdown")`. (02 §S-3)
- [ ] **Document the detcall-IN-reads-zero M0 divergence** (`run_until_hlt` `IoIn`
      arm): every IN — serial and detcall — is filled with zeros, so a detcall IDENT
      reads `0x0`, not `0xD37E0001`; fine because no M0 guest issues a detcall IN. (02 §S-4)
- [ ] **Add a `landing_loop` live test** (`tests/boot_hello.rs`): boot
      `landing_loop_elf()` with a small cmdline iteration count, assert the terminal
      `L` serial byte — cheap (~40ms) coverage of multi-page RAM and cmdline plumbing
      that hello/pipeline_smoke don't exercise. (02 §S-5)
- [ ] **Hoist `kvm-bindings` / `kvm-ioctls` / `vm-memory` to
      `[workspace.dependencies]`** and reference `.workspace = true` from
      `tools/dh-cli/Cargo.toml`, eliminating the version-pin drift with dh-vmm across a
      type boundary. (02 §S-6)
- [ ] **Validate `--mem-mib` at parse time** (`main.rs::boot_cmd`): reject `0` and
      values `> 1024` with the usage message so the failure is immediate and
      self-explanatory rather than a downstream KVM/memfd error or deferred cap
      rejection. (02 §S-7)
