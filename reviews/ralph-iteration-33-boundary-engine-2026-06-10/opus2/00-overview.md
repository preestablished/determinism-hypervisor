# Boundary engine (ARCH §3.2) — independent review (2nd reviewer)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-33-boundary-engine` vs `main`
- **Bead:** `determinism-hypervisor-rng` — "Boundary engine: land at exactly icount N (skid margin + single-step)"
- **Subject:** `crates/dh-vmm/src/boundary.rs` (new, 335 lines) + wiring in `lib.rs`, `Cargo.toml`, `Cargo.lock`
- **Environment:** live `/dev/kvm` rw, `perf_event_paranoid=1`, `nmi_watchdog=0`, `perf_event_max_sample_rate=100000` — every claim below was RUN, not just read.

## Verdict

**LAND IT.** The landing engine is correct, exact, and deterministic under live execution. The
implementation faithfully realizes the §3.2 pseudocode (far PMI approach → near single-step tail),
honors the spurious-EINTR re-read contract from §3.1, parks the PMI period before stepping to dodge
the documented throttle hazard, and drops single-step on every exit path. Repeated runs stayed
bit-exact with zero variance. I found **no Critical or Important defects.** The findings are all
suggestions and doc-clarity notes — none block merge.

## What I verified live (not just by code-walk)

| Check | Result |
|---|---|
| `cargo test -p dh-vmm boundary` x3 (plus `--test-threads=1`) | 4/4 pass every run; 0.27s / 0.31s / 0.43s / 0.44s — **zero flakes** |
| Full `dh-vmm` suite (default parallel = concurrent vCPUs+counters on different threads) | 58/58 pass — thread-safety holds by evidence |
| **Scratch: 50 ascending random targets on ONE guest, exact each time** | PASS, repeated 4x, **zero variance** — no systematic drift across far+near approaches |
| **Scratch: `land_at(target == current counter)`** | Returns immediately, no instruction retired, identical `rip`/`rcx` — `c==target` branch verified LIVE |
| **Scratch: `target == current + 1`** | Exactly one retirement, `rip` advanced — single-instruction landing verified LIVE |
| `cargo clippy -p dh-vmm` | clean (no warnings) |
| Skid timing arithmetic (8174 steps/tail @ 2–4µs) | ~98–196ms for ~6 stepped tails — consistent with observed 0.27–0.44s suite time |

All scratch tests were added in-tree temporarily and **reverted** (`git diff --stat` clean; `grep -c scratch` = 0).

## Stats

- Files changed: 4 (1 new source, 3 wiring)
- New source LOC: 335 (≈215 impl, ≈120 live tests)
- Findings: **0 Critical, 0 Important, 5 Suggestions, 8 Positive notes**
- Live tests in-tree: 4 (all pass, all hardware-gated with graceful skip)
- Reviewer-run torture cases: 3 (50-target, c==target, +1) — all pass

## Files in this review

- `00-overview.md` (this file)
- `01-critical-and-important.md` — none found (with the adversarial cases I tried and why each is safe)
- `02-suggestions.md` — 5 quality/robustness/doc suggestions
- `03-positive-notes.md` — what is notably right
- `04-action-items.md` — copy-paste-ready follow-ups by severity
