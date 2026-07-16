# Positive notes

- **Register discipline is clean and deliberate.** Long-lived state lives in callee-ish
  high registers (`r8` PAD_BASE, `r9` TABLE_GPA, `r10` F) that the inner pace loop never
  touches, while the pace loop confines its churn to `rax`/`rbx`/`r11`/`r12`. This is
  exactly the separation that keeps an unbounded busy loop from corrupting frame state.

- **The serial-echo path was chosen correctly.** Skipping hello's LSR-THRE wait is the
  *right* call here (not a copy-paste omission): `DebugSerial` is output-only and LSR
  always reads ready, so the wait would be pure dead polling. And the PIO `out 0x3F8`
  genuinely reaches `DebugSerial` via `kvm.rs` `SerialOut` classification and the run
  loop's drain — a deterministic, logged sink. The author clearly traced the echo to a
  real destination rather than assuming it.

- **FRAME_COUNTER semantics are understood at the right layer.** The comments correctly
  attribute the FRAME_MARK logging and monotonicity contract to the device + run control
  (a device MMIO handler "cannot FAULT the slot"), and the guest resets F to 0 each boot
  knowing absolute carry-over lives in device state (PADD), not the guest register — the
  subtle but correct interaction with the M5 from-snapshot accept.

- **Drift protection follows the house style.** The new `pad_echo_asm_matches_rust_constants`
  test reuses the exact-token `%define` parsing idiom from the bootinfo/timer_guest/
  rep_loop tests, and pins `PAD_BASE` against the dh-devices window — closing the most
  important asm↔device drift surface.

- **Documentation-in-code is excellent.** Both the asm header and the lib.rs doc comments
  spell out the table layout, the polling-only posture (and *why* no IDT/STI is needed),
  and the pacing rationale. A future reader does not have to reverse-engineer the intent.

- **Plumbing is complete and idiomatic.** build.rs `PROGRAMS`, lib.rs accessor + the
  three consts, and the elf_shape shape-check were all added in the same places as every
  sibling guest — nothing half-wired. Builds clean and all 7 elf_shape tests pass.
