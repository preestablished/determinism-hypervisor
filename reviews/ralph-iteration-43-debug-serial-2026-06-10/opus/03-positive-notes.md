# Positive Notes (patterns to preserve)

- **The iter-29 hazard is closed *and* regression-proven by the guest itself.** `hello.asm` now
  does real 16550 driver discipline (poll LSR THR-empty before each byte). Under the old blanket
  `data.fill(0)` this spins forever; with DebugSerial's LSR=0x60 it makes progress. Encoding the
  fix as a *live boot test* (not just a unit test) is exactly the kind of execution-over-eyeball
  verification this project's review history rewards. (`tests/nanokernel/asm/hello.asm:24-37`,
  `tools/dh-cli/tests/boot_hello.rs:26`)

- **Empty-snapshot contract is deliberate and documented at the seam.** The module doc and the
  `snapshot`/`restore` pair (`serial.rs:1-13`, `:140-159`) state plainly why serial state never
  enters the hash, and `restore` still validates version and rejects non-empty bytes — so the
  "empty" is enforced, not accidental. It composes correctly with `hash.rs`'s
  `(id, version, len, bytes)` framing.

- **IN-FILL contract respected by construction.** Both loops answer serial INs on the raw
  `VcpuExit` slice and match the serial range *before* `classify_exit`, preserving the
  determinism-critical contract documented at `kvm.rs:246`. The boot.rs module comment was
  updated to explain the new behavior accurately rather than left stale.

- **Convention conformance without copy-paste drift.** Device-id is sequential and unique;
  `restore` mirrors the strict `sec_version` + length validation used by `pad.rs`/`clock.rs`/
  `entropy.rs`; the MMIO mirror uses the canonical `0x08 + reg*4` slotting and relies on the bus
  for MAGIC/VERSION rather than re-implementing them.

- **Tests assert the meaningful invariants.** `output_accumulates_and_drains` proves drain
  semantics and that non-THR writes are swallowed; `lsr_polls_ready_and_rx_reads_zero` pins the
  exact LSR/IIR/RBR values a driver depends on; `snapshot_is_empty_and_restore_clears_pending`
  asserts the empty section *and* the strict restore rejections. Good coverage for a 199-line
  module.

- **Pure, host-state-free model.** No `std::time`, no randomness, no IO on any path; `take_output`
  via `std::mem::take` is deterministic. The crate's `no_host_ambient_authority` grep gate covers
  the new file automatically.
