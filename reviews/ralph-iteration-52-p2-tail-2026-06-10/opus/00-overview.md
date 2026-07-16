# Review Overview — ralph/iteration-52-p2-tail (P2 tail: nq5 + 4ld)

- Reviewer: Claude Opus
- Date: 2026-06-10
- Branch: `ralph/iteration-52-p2-tail`
- Diff base: `main` (`git diff main...HEAD`)
- Verdict: **APPROVE** (with one Important follow-up bead and two suggestions; nothing blocks merge)

## Scope

Two beads land in one iteration:

- **nq5** — `config::CpuidLeaf` gains `flags: u32`; new `CpuidLeaf::encode_into`
  becomes THE single canonical leaf encoding; new `config::cpuid_leaves_hash`;
  `MachineConfig::canonical_encode` and `cpuid::cpuid_table_hash` both route
  through it; new `cpuid::to_leaves` bridge (the 8jx wiring seam); golden
  canonical-bytes test consciously updated (+4 bytes/leaf for `flags`).
- **4ld** — `dhilog` `KIND_ENCODER_FP = 0x46` AUX record +
  `LogWriter::encoder_fingerprint`; `DevCtx::log_encoder_fingerprint` wrapper;
  `detchannel::wire_encoder_fingerprint()` (digest8 over a fixed 4-probe set),
  emitted once at every successful `CHANNEL_INIT` attach; m1_acceptance record
  count 5 -> 6.

## What I ran (Intel lab box, /dev/kvm rw)

- `cargo build -p dh-cli` + `dh-cli cpuid-diff` live — **masked table hash =
  `f19610e179617f2c8f103d1bf2d6791ffb63b3d4876d254b81bb16033fb4738e`**, byte-for-byte
  IDENTICAL to the committed artifact `docs/ops/cpuid-diff-infra-control.txt`.
- Scratch integration test wiring `to_leaves(masked_cpuid())` into a
  `MachineConfig` and calling `validate()` — **PASSES** today (40 leaves, 0
  duplicate (function,index) pairs, 16 SIGNIFICANT_INDEX entries). Scratch file
  removed; tree clean.
- `cargo test -p dh-inputlog -p dh-devices -p dh-vmm` — all green (74 + fixtures).
- `cargo test -p determinism-tests --test m1_acceptance` x3 (incl. the test's own
  built-in run-twice bit-identical compare) — all green.
- `cargo test --workspace` — all green, 0 failures.
- `cargo clippy --workspace --all-targets` (x86_64) — clean.
- `cargo clippy --workspace --all-targets --target aarch64-unknown-linux-gnu`
  (clang/llvm-ar-18/a64inc env) — clean.
- Tree clean after (`git status` shows nothing to commit).

## Headline findings

1. **nq5 preimage unity is sound and hash-stable.** The OLD `cpuid_table_hash`
   already serialized `function,index,flags,eax..edx` in that order; the new
   `encode_into` uses the SAME order, so the masked-table hash is unchanged and
   the committed artifact still matches reality. No second leaf encoding remains
   anywhere in the tree. Continuity holds even though it isn't required pre-M4.

2. **4ld probe set is too narrow.** The digest path (`sdk_event_digest` ->
   `wire_view`) can re-encode **14** `EventPayload` variants; the fingerprint
   probe covers only **3 of those 14** (Hello, NameIntern, Beacon) plus `Pad`
   (which is never a drained payload). A wire-format change to any of the other
   11 variants (AssertViolation, Reachable, InjectQuery, RegionRegister,
   RegionUpdate, WorkloadStarted, WorkloadExited, LogLine, QuiesceReady,
   FrameMark, Ready) would skew SDK digests WITHOUT flipping the fingerprint —
   the exact false-negative the bead exists to prevent. Extending the probe to
   all 14 is cheap and is the right call. Filed as the one Important follow-up.

3. **restore() emits no fingerprint.** Emission is tied to the live attach
   (`PORT_INIT_GO`), not to segment start; `restore()` re-attaches via
   `Channel::attach` and has no log handle, so a post-restore segment carries
   SDK_EVENT digests but no fingerprint. Design-level concern for M4 replay, not
   a defect today (no replayer exists yet). Suggestion.

See `01-critical-and-important.md`, `02-suggestions.md`, `03-positive-notes.md`,
`04-action-items.md`.
