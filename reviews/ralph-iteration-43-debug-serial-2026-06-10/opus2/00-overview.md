# Review: iteration-43 debug-serial device (ARCH §6.9)

- **Branch:** `ralph/iteration-43-debug-serial`
- **Date:** 2026-06-10
- **Reviewer:** Claude Opus (2nd reviewer)
- **Commit:** `166d9e6` (one commit vs `main`)
- **Bead:** avm — debug-serial: 16550-subset, PIO 0x3F8 + MMIO mirror, output-only

## Summary

This iteration adds `DebugSerial` (`crates/dh-devices/src/serial.rs`), an output-only
16550 subset: OUT bytes are buffered for host observability, RX reads return 0, LSR
always reads transmitter-ready (0x60), config-register writes are swallowed and read back
as fixed constants, and the snapshot section is empty by design (serial never enters the
state hash). The model is wired into both dh-cli debug loops — `boot.rs` `run_until_hlt`
(replacing the iter-29 blanket IoIn zero-fill for the serial range) and `run.rs`
`run_segment`'s `on_exit` — and `hello.asm` now polls LSR (bit 5) before every byte as
live proof that an LSR-polling driver makes progress instead of spinning forever.

I executed the suite and wrote three throwaway live experiments on the lab box (/dev/kvm
present): (1) the **IN-at-boundary determinism** experiment — ran the LSR-polling hello
guest to seven exact icount targets spanning the poll/IN/OUT region, twice each; every
target landed at the identical (icount, rip, state_hash, serial) tuple, proving serial
INs near and at boundaries neither double-count nor break the §3.2 landing engine
(answering the highest-risk angle, #5). (2) An **exit-budget count** — hello consumes 13
exits (6×IN + 6×OUT + 1 HLT), far under the 10_000 budget; the poll succeeds on the first
IN every byte exactly as the LSR-always-ready design intends (#6). (3) A **device_sections
framing** check — the empty serial section round-trips as `id=0x0006 ver=1 len=0` (8 bytes)
and stays self-delimiting alongside a second device (#1). `DEVICE_ID_DEBUG_SERIAL = 0x0006`
is unique across all `DEVICE_ID_*` in the workspace.

The code is correct, deterministic, well-tested at the unit level, and the design posture
is sound. The findings are not defects in the shipped behavior — they are latent
future-integration traps and maintainability gaps. The most notable is that the real
IN-fill seam is the *raw* `VcpuExit::IoIn` (as both debug loops correctly use), while
dh-vmm's `classify_exit` already carries `ExitEvent::SerialIn { port, len }` that drops the
mutable buffer — a future M1 hashed run loop built on `classify_exit` cannot fill serial
INs through that path and must intercept the raw exit, exactly like the iter-29 hazard.
This deserves a documentation guard given the history.

## Verdict

**APPROVE**

No Critical or Important defects in the diff. One Important *documentation* item to
forestall a repeat of the iter-29 hazard; the rest are suggestions.

## Stats

- Files changed: 7 (+244 / −16); 1 new file (`serial.rs`, 199 lines incl. tests)
- New tests: 4 unit tests in `serial.rs` (all pass)
- Suites run green: `dh-devices` (65), `dh-cli` (all incl. `boot_hello`), `dh-vmm` hash (6),
  `nanokernel` (5); `cargo clippy -p dh-devices` clean
- Live experiments written + run: 3 (IN-at-boundary determinism, exit count, framing) — all
  confirmed expected behavior; experiment files removed afterward
- Findings: 0 Critical, 1 Important (doc), 6 Suggestions
