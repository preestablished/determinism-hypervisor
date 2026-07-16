# Positive Notes

- **The core §3.1 empiric is correct and well-grounded.** "VM-exiting instructions retire zero under `exclude_host=1`; KVM completes them host-side via RIP skip" matches the constants (`COUNTING_DELTA_AT_OUT_EXITS = 1000 - 3 = 997`), the build-enforced `EXITCOUNT == 3` in the asm, and the live `counting_smoke` test (997, bit-identical across two cold boots). Replacing the refuted "retire exactly once on the completing resume" with the measured rule is exactly the right call, and keeping the historical note ("an earlier revision claimed … the empirics refuted that") is good spec hygiene.

- **The per-determinism-class framing is preserved.** The new §3.1 correctly mirrors the interrupt rule's "re-validate per class, never assume across classes" discipline rather than presenting 997 as a universal constant — consistent with the constant's own doc (`lib.rs:116-135`) and the smoke test's class-locked phrasing.

- **The guest-sdk ring-W reconciliation is exactly right and fully cross-checked.** `0x1E0000 → 0x100000` matches `../guest-sdk/crates/detguest-wire/src/header.rs:103` (`RING_W_SIZE = 0x10_0000`), the power-of-two `const_assert` (`header.rs:118`), and the contiguity asserts (`header.rs:122-123`). The arithmetic is clean: ring A `0x010000 + 0x10000 = 0x020000`; ring W `0x020000 + 0x100000 = 0x120000`; reserved `0x120000 → 0x200000`. The new `0x120000 reserved` row matches `OFF_RESERVED_TAIL` (`header.rs:107`). `DEVICE_EXERCISE_RING_DESCS` W = `(0x2_0000, 0x10_0000)` agrees, and `channel_interop` passes — so the asm, the constants, the vendored table, and the real detguest-host attach validation are now all consistent.

- **§3.2 boundary pseudocode was left consistent.** The "MMIO etc. are serviced; they don't disturb counting" line (`:260`) still holds under the new zero-retirement rule — no contradiction introduced. (Minor wording nit in 02-S3.)

- **`lib.rs` comment updates are accurate and the more careful of the comment edits.** Both the `COUNTING_DELTA_AT_OUT_EXITS` doc (`:122-135`) and the `DEVICE_EXERCISE_RING_DESCS` doc (`:145-151`) now phrase the doc state correctly ("the vendored doc now records the measured rule"; "fixed in the vendored copy, upstream tracked") — past-tense and scoped to vendored-vs-upstream. This is the right model; the asm comments should match it (I1, S1).

- **No code regression.** Full `cargo test --workspace` passes (KVM live), `cargo clippy --workspace --all-targets` is clean, working tree clean. This is a genuinely low-risk, mostly-accurate documentation pass.

- **TIMER_DEADLINE = absolute guest vns** (the first half of the §6.2 edit) is correct against `clock.rs` — the register and `armed()` both operate on the continuous `vns_base` axis. Only the "run control subtracts internally" mechanism clause is wrong (I2).
