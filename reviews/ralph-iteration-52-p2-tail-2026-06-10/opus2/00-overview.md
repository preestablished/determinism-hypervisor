# Review — iteration 52 P2 tail (nq5 + 4ld)

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** ralph/iteration-52-p2-tail
- **Diff:** `git diff main...HEAD` (6 files, +142 / -26)
- **Verdict:** **NEEDS_DISCUSSION**

## Scope

Two beads landed as a "P2 tail" cluster:

- **nq5** — `CpuidLeaf` gains a `flags: u32` field; the per-leaf byte
  encoding is extracted into the single canonical `CpuidLeaf::encode_into`,
  now shared by `MachineConfig::canonical_encode` and a new
  `config::cpuid_leaves_hash`, which `cpuid::cpuid_table_hash` calls via a
  new `cpuid::to_leaves` bridge. The golden config vector grows +4 B/leaf.
- **4ld** — A new `KIND_ENCODER_FP = 0x46` AUX record. `LogWriter::
  encoder_fingerprint` writes an 8-byte fingerprint; `DevCtx::
  log_encoder_fingerprint` stamps it at the boundary; `detchannel`'s
  `wire_encoder_fingerprint()` computes a `digest8` over a fixed 4-probe
  canonical encoding set and emits it at every successful `PORT_INIT_GO`
  attach. `m1_acceptance` record count 5 → 6.

## Build / test / lint (all RUN on the lab box)

| Gate | Result |
|------|--------|
| `cargo test -p dh-vmm --lib` | 74 passed |
| `cargo test -p dh-devices --lib` | 67 passed |
| `cargo test -p determinism-tests --test m1_acceptance` | 1 passed (6-record assert) |
| `cargo test --workspace` | all green, 0 failed |
| `cargo clippy --workspace --all-targets` (x86_64) | clean, 0 warnings |
| `cargo clippy --workspace --all-targets` (aarch64) | clean, 0 warnings |

aarch64 ran with the prescribed env (`CC/CFLAGS/AR_aarch64_unknown_linux_gnu`,
`-isystem /tmp/a64inc`). Working tree clean after the run (only the new review
files added).

## Why NEEDS_DISCUSSION, not APPROVE

The code is correct, builds, and passes everything. nq5 is genuinely good —
it *closes a pre-existing fork* (on `main`, `cpuid_table_hash` already hashed
`flags` while `canonical_encode` did not; see 03). 4ld's mechanics are sound.

But 4ld makes a **design decision that is wrong for the stated invariant**,
and it is wrong silently: the fingerprint is emitted from the *device attach
event* (`PORT_INIT_GO`), not from the *segment/log*. The DHILOG doc text 4ld
itself adds says the record is "emitted ONCE per segment at channel attach" —
yet the EVTC snapshot/restore path (iteration 42) re-attaches the channel
*without* going through `PORT_INIT_GO`, so a segment that begins from a restore
gets **zero** fingerprint records. The per-segment invariant the comment
asserts is violated by the very restore path this codebase already ships.

This is forward-looking plumbing (no verifier consumes the record yet), so it
is not breaking anything today — which is exactly why it should be discussed
*now*, before a consumer is written against the wrong placement. See 01,
finding C1, for the full M4-replay walk and my recommendation on where the
emission belongs.

One real (minor) bug also landed: a misattributed doc comment in `ctx.rs`
(C-IMPORTANT I2) left `log_frame_mark` undocumented and gave the new fn the
wrong first doc line. Trivial to fix.
