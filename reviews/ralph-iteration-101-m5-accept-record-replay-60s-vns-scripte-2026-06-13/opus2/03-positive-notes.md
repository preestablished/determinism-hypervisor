# Positive Notes — patterns worth preserving

## P1 — The test delegates verification to the production contract instead of re-implementing it

`replay_once` (lines 326-364) drives the real `replay_segment` and asserts on its *returned outcome*
(`records_applied`, `epoch_hashes_verified`, `end_icount`, `end_state_hash`). The chain math, epoch
linking, and divergence detection all live in `replay_engine.rs` and are merely pinned here. This is
exactly the "exercise the contract, not re-derive production logic" discipline your research flags;
there is no re-implemented hash chain that could drift from production and mask a real bug.

## P2 — The seeded script is genuinely deterministic across platforms and compilers

`splitmix64` (lines 111-118) uses only `u64` wrapping add/mul/xor/shift — all well-defined,
endianness-independent, and identical on every Rust target. `pad_script` (lines 123-126) is a pure
function of the fixed `SCRIPT_SEED`. There is no float, no hash-map iteration order, no clock, no
allocation-address dependence. The "seeded, scripted" wording in the acceptance is honestly earned:
the input sequence is bit-reproducible anywhere the test compiles, which is the right foundation for
a determinism acceptance gate.

## P3 — The two `#[test]` fns are safe to run in parallel threads

Each test opens its **own** per-thread counter (`InstRetired::open_for_current_thread`,
lines 181/417/450) and routes overflow to its **own** tid (`route_overflow_to_thread(gettid(), …)`).
The kick handler is process-wide but idempotent (`install_kick_handler`, run.rs:44 — "install
process-wide; idempotent") and the kick *target* is a `thread_local!` `Cell` (run.rs:36-40), so one
test's vCPU registration cannot steal the other's signal. The snapstore is a per-test `TempDir` +
fresh side runtime (`spawn_store_blocking`, common/mod.rs:37-46) with a distinct UDS. KVM slots are
created per test from a freshly `KvmSystem::open()`'d handle. I found no shared mutable state, no
shared counter, no shared socket, and no ordering dependency between `m5_smoke_…` and
`m5_accept_…`. They are safe under cargo's default parallel test harness.

## P4 — The recorder self-validates the log it produced before handing it to replay

After sealing, `record` re-parses its own log (lines 296-312) and asserts: epoch flag set, exactly
`seconds` EPOCH_HASH records, recorded PAD_SET sequence **byte-equals the script** (line 310, *not*
dedup'd — the strong form), and the log's END hash equals the live `end_state_hash`. This catches a
recording-side fault *before* it can masquerade as a replay success, and the non-dedup'd `pads ==
script` compare is the real proof that every scripted input landed.

## P5 — The grid invariants are pinned, not assumed

`QUANTUM == epoch_len == 100_000` is asserted to *land exactly* (`out.boundary.icount == i * QUANTUM`,
line 263-267) at every quantum, and the final vns identity (`seconds * 1e9`, line 285) is checked
exactly rather than approximately. The `count < CAPACITY` guard in `assert_table_eras` (line 377)
correctly refuses to reconstruct eras from a wrapped ring — a lossy read would otherwise pass
silently. These turn implicit assumptions about the absolute epoch grid into loud, testable facts.

## P6 — Honest, load-bearing comments

The module header (lines 1-38) and inline comments explain *why* the non-1:1 clock matters, why the
file lives in `dh-worker/tests` rather than `tests/determinism` (ARCH §1), why x100 is `#[ignore]`d,
and why a fresh slot per replay is used. The `entropy_seed: [0; 32]` header comment (line 156) and
the "zero ⇒ replay continues the snapshot PRNG" note correctly match the engine's §3.1 behavior
(replay_engine.rs:133-138). The comments would let a future maintainer reconstruct the design intent
without archaeology.
