# Positive Notes

### P1. The determinism claim is tested against the REAL device path, not a stub

The doorbell goes through `PvEntropy::doorbell` (entropy.rs:122) → `ctx.entropy.fill()` →
`ctx.mem.write(buf_gpa)` → `ctx.log_entropy()`, the same code M1 uses. The test does not
re-implement the PRNG or pre-compute expected bytes — it uses the un-snapshotted
continuation **as** the golden. That neatly avoids the research note's #1 pitfall
("tautological tests that re-implement production logic and assert it against itself"): the
reference and the replay are produced by the *same* device + PRNG, and the claim is purely
"restore preserves position," which is exactly the contract under test.

### P2. HLT classification and the batch boundary are exact, zero-skid

`run_segment` maps `VcpuExit::Hlt` to `GuestHalted` (runctl.rs:241) and `finish_halted`
reads the true icount/RIP (runctl.rs:418-444). Using `Until::IcountBudget(1e9)` means the
far-approach PMI is armed ~1e9 retirements out while the guest HLTs after a few thousand —
so `land_at` never enters the single-step near-approach, and the iteration-50 MMIO-write
trap-eating hazard (bead 4a3) is structurally avoided, not merely worked around. The asm
header comment and module doc both explain this accurately.

### P3. Full GPR round-trip is real and correctly relied upon

`capture` → `get_regs()` (the entire `kvm_regs`, r8–r15 included) → `encode_section` via raw
`struct_bytes` → `decode_section` → `restore` → `set_regs` (vcpu_state.rs:126/155/244/271).
r8 (ENT_BASE=0xD000_3000) and r9 (TABLE_GPA=0x50_0000), live at the snapshot RIP
(`jmp .batch`), survive the restore, which is precisely why leg B's guest keeps drawing
into the correct ring with the correct device window. The drift test pins ENT_BASE ==
0xD000_3000 to the bus convention (elf_shape.rs:530).

### P4. GPA layout is collision-free and bounded

Ring = header(8) + 2^15·16 = `0x580008`, matching the asm comment. `[0x50_0000, 0x58_0008)`
clears the guest image (0x10_0000), timer table (0x20_0000), pad_echo (0x30_0000), and the
device-exercise channel (0x40_0000), and stays inside 16 MiB RAM. The `RING_MASK = 0x7FFF`
wrap means the ring never grows unbounded regardless of run length, and `read_draws`'s
`(from+i) & (CAPACITY-1)` mirrors the guest's slot math exactly. The count is bumped
**after** the device fills the slot (asm:408-409), giving a host sampler torn-read
discipline.

### P5. MMIO access widths line up exactly with the device's match arms

8-byte `mov [r8+REG_BUF_GPA], rdx` → `(REG_BUF_GPA, 8)` (entropy.rs:160); 4-byte
`mov dword [r8+REG_DOORBELL], 1` → `(REG_DOORBELL, 4)`; 4-byte `mov eax, [r8+REG_STATUS]`
→ `(REG_STATUS, 4)`. No width mismatch that would silently hit the `_ => {}` / `fill(0)`
fallbacks. The one-time `mov [r8+REG_LEN], eax` programs LEN=16 before the loop, matching
`(REG_LEN, 4)`.

### P6. The snapshot attestation is honest

`take_snapshot` trusts the caller's `BoundaryState` and `SlotState` (snapshot_engine.rs:113-117).
The slot really is at a deterministic instruction boundary (post-HLT, RIP at `jmp .batch`,
IF=0 — not mid-instruction), the boundary engine owns the Paused transition for a
GuestHalted segment, and `icount`/`vns`/`hash_chain`/`agenda_empty` are sourced from the
actual `a1` outcome (`a1.boundary.icount`, `a1.vns`, `chain_a.value()`, `true`). With clock
1:1, `vns == icount`, consistent. Passing `SlotState::Paused` literally is therefore not a
fib.

### P7. Log writer capacity and ordering are non-issues

`LogWriter` appends via `self.buf.push` (dhilog.rs:346) — a growable Vec, so 1536 (leg A)
and 1024 (leg B) ENTROPY records are trivial. The §3.2 monotone-watermark contract is
satisfied because every `DevCtx::new(icount, …)` stamps `ctx.entropy()` with the
freshly-read counter (entr_golden.rs:180-183), which is monotone within each LogWriter (the
counter never resets within leg A; leg B uses a fresh `log_b`). `ctx.log_fault()` is checked
after every dispatch (entr_golden.rs:193-195) — correctly wired.

### P8. The harness self-skips cleanly and the drift pins are thorough

`kvm_available()` gate (entr_golden.rs:217), HARDWARE-GATED doc, and a comprehensive
`entropy_draw_asm_matches_rust_constants` drift test (elf_shape.rs:497-531) that pins
**every** asm `%define` against the Rust mirror *and* the device-side register truth from
`dh_devices::entropy`. The `golden.iter().any(|b| *b != 0)` guard (entr_golden.rs:288-291)
catches the "doorbell never ran" degenerate case before the equality assert, so an all-zero
== all-zero false pass is impossible.
