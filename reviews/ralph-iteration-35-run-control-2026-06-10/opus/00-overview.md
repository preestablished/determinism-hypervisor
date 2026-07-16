# Review: iteration-35 run control (Phase-1 Run segment)

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-35-run-control` vs `main`
- **Beads:** determinism-hypervisor-qs4 (Phase-1 run control), resolves srz (Margins from MachineConfig)
- **Commit:** feaf237

## Verdict

**Request changes.** The feature is well-built, fully live-tested, and deterministic
for every path it exercises today — but it ships a **latent correctness bug in
multi-vector injection chaining (#3)** that will silently lose interrupt vectors the
moment the device/timer loop schedules two vectors at one boundary. Phase-1 never hits
that path (the only caller passes `injections: &[]`), so nothing in the current test
matrix catches it, but the code *claims in a comment* to chain vectors and does not. It
must be fixed (or the claim removed and the multi-vector case rejected loudly) before
the device loop lands, and a live two-vector test added now while the boundary engine is
fresh.

Two additional spec-conformance gaps are Important: `hash_epochs = FinalOnly` is silently
ignored (#1), and a point that is both an epoch boundary and the final stop produces **two**
chain links instead of one (#5) — both are interop hazards for the §8.5 chain even though
each is internally deterministic.

Everything else is correct: the pause roll-forward math is sound, the agenda walk is a
faithful compilation of §3.3, the Until/StopReason enums align with API.md §2.4's
Phase-1 subset, and the run-twice determinism claim holds byte-for-byte.

## Statistics

| Metric | Value |
|---|---|
| Files added | 2 (`crates/dh-vmm/src/runctl.rs`, `tools/dh-cli/src/run.rs`) |
| Files modified | 4 (lib.rs ×2, main.rs, Cargo.toml/lock) |
| New runctl LOC | 446 |
| Findings: Critical | 1 |
| Findings: Important | 2 |
| Findings: Suggestions | 5 |
| Positive notes | 7 |

## Verification performed

- `cargo test -p dh-vmm runctl::` — **4 passed** (icount budget run-twice, goal poll,
  pause roll-forward, unwired modes), all LIVE against `/dev/kvm` (rw, group `kvm`),
  `perf_event_paranoid=1`.
- `cargo test --workspace` — **27 suites, all green**, 0 failures (dh-vmm 66 unit tests
  incl. the 4 new ones).
- `cargo clippy -p dh-vmm -p dh-cli` — clean, no warnings.
- `dh-cli run landing_loop.elf --icount-budget 500000` run **twice** → **byte-identical
  JSON** including `state_hash`
  (`5398e78d…baa06fbf`), `reason=budget_reached`, `icount=500000`, `vns=500000`.

All NORMATIVE sections read: ARCHITECTURE §3.3, §3.4, §8.5; API.md §2.4; agenda.rs and
its module docs; boundary.rs; inject.rs; config.rs; hash.rs.
