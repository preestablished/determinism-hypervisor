# Review Overview — DHSNAP EVTC detchannel host state save/restore

- **Reviewer:** Claude Opus
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-42-evtc-snapshot` vs `main`
- **Bead:** determinism-hypervisor-e7y
- **Scope:** `crates/dh-devices/src/detchannel.rs` — `DetChannelHost::snapshot()` / `restore()` for the DHSNAP `EVTC` section contents (+ `evtc_tests`)
- **Diff size:** ~257 added lines, one file

## Verdict

**APPROVE WITH MINOR FOLLOW-UPS.**

The change is correct, well-tested, and faithful to the normative contracts
(guest-sdk `channel.rs` reconstructible-vs-not split; ARCHITECTURE §8.3 restore
order). The byte layout is internally consistent (writer ↔ reader walked
byte-for-byte; `EVTC_LEN = 39` confirmed) and restore is genuinely atomic on the
failure path. No correctness bug found. The follow-ups are: an undocumented
non-serialized-state gap (the responder's `FaultPlan` accumulators), permissive
flag-byte decoding, and a minor turbofish API ergonomics wart — none blocking.

## Stats

| Category    | Count |
|-------------|-------|
| Critical    | 0     |
| Important   | 1     |
| Suggestions | 3     |
| Positive notes | 5  |

## Quality gates (run on the branch)

| Gate | Result |
|------|--------|
| `cargo test -p dh-devices` | PASS — 61 unit + 10 smoke + 2 new `evtc_tests`, 0 failed |
| `cargo clippy -p dh-devices --all-targets` | PASS — no warnings |
| `cargo fmt -p dh-devices -- --check` | PASS — clean |

## What was verified directly

- **Layout arithmetic:** writer order (init_lo/hi/status u32s; inject flag@12+u32@13;
  quiesce flag@17+u32@18; channel flag@22 + gpa@23..31 + seq_c@31 + seq_i@35)
  matches the restore reads exactly. `EVTC_LEN = 4+4+4+5+5+1+16 = 39`. Confirmed.
- **Restore atomicity:** `Channel::attach(...)?` returns early on `Err` *before* any
  `self.*` field is assigned — a failed attach leaves `self` fully unchanged. The
  `manifest_read_failures` bump only occurs on the successful-attach path. Confirmed.
- **Seq non-reuse:** the attached roundtrip test pushes twice pre-snapshot, restores,
  and asserts the next push yields `ring_c == seqs_before.ring_c + 1` — the exact
  replay hazard `ProducerSeqs` exists to prevent. Confirmed against
  `guest-sdk channel.rs` `producer_seqs`/`restore_producer_seqs`.
