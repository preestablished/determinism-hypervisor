# DHILOG v1 Reader / Replay Iterator — Review Overview

- **Branch:** `ralph/iteration-61-dhilog-v1-reader-replay-iterator` vs `main`
- **Date:** 2026-06-11
- **Reviewer:** Claude Opus
- **Bead:** ecv (validating read side of API.md §3)

## Summary

This change adds the validating read side of DHILOG v1: `crates/dh-inputlog/src/reader.rs`
(`LogReader::parse` as a total decoder, a full §3 validation battery, infallible typed
iteration, and the AUX-skipping contract), three new spec constants in `dhilog.rs`
(`KIND_NET_RX`/`KIND_EPOCH_HASH`/`KIND_NET_TX`, `FLAG_EPOCH_HASHES`, `MAX_NET_RX_FRAME`),
and a 25-test validation battery. The parse path is genuinely panic-free over untrusted
bytes: I traced every slice index in `parse_header`, `validate_records`, `validate_kind`,
`Records::next`, and `Record::body` back to a dominating bounds check, and every
known-kind payload offset accessed in `body()` is guaranteed by a `validate_kind` length
assertion that runs before `body()` can ever be reached on a parsed record. Integer
arithmetic on the attacker-controlled `payload_len` cannot overflow (capped at 4096 before
any addition). Spec fidelity against §3.1/§3.2/§3.3 is high, and the validation battery is
well-targeted (the reseal helper correctly puts the body-hash gate behind the rule under
test). The one real robustness gap is that `Record` exposes all-public fields plus a
public `body()` whose doc-comment promises infallibility — a caller can hand-construct a
`Record { kind: KIND_END, payload: &[], .. }` and `body()` will panic, because the
infallibility invariant lives in `parse()`, not in the type. That is an API-surface hazard,
not a parse-path bug. There is also a documented (and correct-vs-writer) divergence from
the API.md §3.1 table over the `[240..248)` encoder-fingerprint vs `[248..256)` reserved
split that should be fixed in the spec doc.

## Verdict

**APPROVE** (with one Important follow-up: harden `Record::body()`'s public contract, and
update the API.md §3.1 reserved-field row).

The parse path — the security-sensitive surface — is correct and total. No Critical issues.
The Important item is about the public API contract of `body()` on a forged `Record`, which
does not affect any in-tree caller today but contradicts the method's own documentation.

## Stats

- Files changed: 4 (`reader.rs` new +498, `dhilog.rs` +9, `lib.rs` +1, `reader_validation.rs` new +469)
- Tests: 25 new integration tests + 10 existing writer unit tests — all 35 pass
- Clippy: clean (`cargo clippy -p dh-inputlog`, no warnings)
- Critical: 0 | Important: 1 | Suggestions: 6
