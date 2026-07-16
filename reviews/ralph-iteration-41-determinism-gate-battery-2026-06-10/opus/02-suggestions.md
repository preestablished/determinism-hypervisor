# Suggestions

### S1. `dh-cli gate` argument parsing silently ignores malformed `--runs` and the bare-N form

**File:** `tools/dh-cli/src/main.rs` (the `Some("gate")` arm).

```rust
let runs = args
    .get(1)
    .and_then(|a| (a == "--runs").then(|| args.get(2)).flatten())
    .and_then(|v| v.parse().ok())
    .unwrap_or(100);
```

Every failure path collapses to the 100 default:
- `dh-cli gate --runs abc` → parse fails → 100 (silent; a typo'd number runs the full 100-run,
  ~32s gate instead of the intended quick check).
- `dh-cli gate --runs` (no value) → 100.
- `dh-cli gate 5` (no `--runs` flag) → 100 — a plausible user mistake given the `skid
  [--samples N]` sibling uses the same flag style, so muscle memory is the only guard.
- `dh-cli gate --jobs 5` (wrong flag) → 100, no error.

For a tool whose whole point is a 32s+ live gate, silently defaulting a fat-fingered `--runs`
to 100 is a mild footgun. Suggest: if `args.get(1) == Some("--runs")` but the value is
missing/unparseable, print usage and `exit(2)` rather than defaulting. The `skid` arm has the
identical pattern, so consider a tiny shared `parse_count(args, "--runs", default)` helper —
out of scope for this bead but worth a cleanup bead.

### S2. timer-event sub-gate passes a meaningless cmdline to `timer_guest`

**File:** `tools/dh-cli/src/gate.rs`, `cold_fingerprint` — `load_and_enter(..., b"1000000000")`.

The cmdline `"1000000000"` is reused for both the landing loop and `timer_guest`. For the
landing loop it presumably scales iterations; for `timer_guest` the first byte selects the mode
(`m`/`a`/`d`), and `'1'` matches none, so it falls through to `.open_window` (STI default) —
functionally correct, but the cmdline is inert noise for the timer guest. A reader of
`cold_fingerprint` sees a magic `"1000000000"` handed to a guest that ignores it. Suggest
either passing `b""` (or an explicit mode byte) when `timer.is_some()`, or a one-line comment
that the timer guest ignores cmdline and only the landing loop reads it. Pure clarity; no
behavior change.

### S3. `read_table` panics (`.unwrap()`) instead of returning a gate error

**File:** `tests/determinism/tests/common/mod.rs`, `Rig::read_table`.

`read_slice(...).unwrap()` on both reads. If guest RAM read ever failed (it won't under the
current rig, but the rest of the rig is scrupulously `Result`-threaded with `map_err`), it
panics out of the `zero_divergence` closure rather than failing the gate with a diagnosable
message. For consistency with `boot`/`run_one`, consider returning `Result<(u64, Vec<u8>),
String>` so a read failure becomes a gate FAIL line in the artifact rather than a backtrace.
Low priority — the GPA reads are infallible in practice.

### S4. Split the +130s kvm-intel cost into a nightly lane as the suite grows (future bead)

**File:** `.github/workflows/ci.yaml` kvm-intel lane.

Two 100-run cold-boot tests add ~130s to *every push* on the self-hosted box (per the commit
message; the box is the long pole and shared). That is justified today — these two tests *are*
the M3 determinism acceptance, and the phase doc forbids starting M4 until this gate is green,
so on-push is the right place for them now. But the determinism-regression family is going to
grow (8n7's 1e9 run-twice, the landing-precision 10k-target test 8g1, counting-semantics gfb,
the M5 100× record/replay). The plan itself already anticipates a push-vs-nightly split
(IMPLEMENTATION-PLAN line 156: "(a) run-twice-compare-hash on every PR; (b) nightly: ...").
Suggest filing a bead to split the heavy 100-run determinism tests to a nightly + required-
status lane once the on-push budget crosses a threshold (e.g. >5 min), keeping a fast smoke
(small N) on push. Not actionable this iteration.

### S5. Reduce duplication between `dh-cli/src/gate.rs::cold_fingerprint` and `common/mod.rs::Rig::boot`

Both reimplement the same boot dance (install_kick_handler → KvmSystem::open → create_slot_vm →
load_and_enter → InstRetired open/route/arm/reset/enable → MachineConfig). They differ only in
the kernel-hash seed (`[9;32]` vs `[3;32]`), the cmdline, and that `cold_fingerprint` runs a
single fixed segment while `Rig` exposes per-segment `run_one`. The `Rig` already lives in the
test tree, not in `dh-cli`, so sharing is non-trivial (would need a small lib home). Noting it
as a future consolidation target rather than a request — the duplication is currently
intentional (the rig is test-only, the CLI is shippable) and small. If a third caller appears,
extract a `dh-vmm`-side `cold_boot(elf, cmdline, seed) -> Rig`-like helper.

### S6. Document the "delivered list" vs "ISR table count = FIRES-1" relationship in the test, not just the commit

**File:** `tests/determinism/tests/timer_determinism.rs`.

The inline comment ("the final segment's still-queued vector — budget == deadline merges the
points") is good and correct. One sentence would make it airtight for a future reader: the
fingerprint compared across runs is the **delivered-icount list** (length 10, all deadlines),
while the **ISR table** observes only 9 because each segment's queued vector executes on the
*next* segment's first entry and the 10th has no successor. Both numbers are deterministic; they
just count different things (queued vs retired). The asymmetry is the kind of thing that looks
like a bug on a cold read. Optional.
