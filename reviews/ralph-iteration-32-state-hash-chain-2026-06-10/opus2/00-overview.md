# StateHashChain (Phase 1 / M3) — Independent Review

- **Reviewer:** Claude Opus (2nd reviewer)
- **Date:** 2026-06-10
- **Branch:** `ralph/iteration-32-state-hash-chain` vs `main`
- **Bead:** 35z
- **Scope:** `crates/dh-vmm/src/hash.rs` (new, 476 lines), `crates/dh-vmm/src/lib.rs` (+1 line: `pub mod hash;`)
- **Angle:** hash-preimage pedantry + KVM state-capture depth (deliberately not mirroring the first reviewer)

## Verdict

**APPROVE WITH NITS.** The module faithfully implements ARCH §8.5 for the M3 run-twice-compare,
field-by-field serialization avoids padding nondeterminism, the §8.1 non-XSAVE subset is captured
correctly, and the Phase-1 scoping (full-memory walk, normalized TSC, XSAVE deferral) is sound and
explicitly documented. Crucially, **the two live tests genuinely executed on this host's `/dev/kvm`
(verified: zero "skipping" lines), not skipped** — which proves `KVM_GET_MSRS` returns all 14 entries
including `SPEC_CTRL` on this box. No Critical defects. The findings below are forward-compatibility
and preimage-hygiene items worth resolving while `dh-statehash-v1` is young.

## Test execution (live, this host)

```
$ cargo test -p dh-vmm hash -- --nocapture
running 7 tests
test hash::tests::h0_is_deterministic_and_input_sensitive ... ok
test hash::tests::links_chain_and_every_input_matters ... ok
test hash::tests::out_of_order_pages_panic - should panic ... ok
test hash::tests::vcpu_blob_is_stable_across_reads_live ... ok    # LIVE — ran, not skipped
test hash::tests::final_link_sees_guest_ram_live ... ok           # LIVE — ran, not skipped
test result: ok. 7 passed; 0 failed; 0 ignored
$ cargo test -p dh-vmm hash -- --nocapture 2>&1 | grep -ci skip   →  0
```

`/dev/kvm` is `rw` for this session (`crw-rw---- root kvm`, user in `kvm` group). The live tests call
`canonical_vcpu_blob`, whose `get_msrs` path returns early with an error if `n != 14`; both live tests
passed, so the 14-entry GET (incl. `SPEC_CTRL` 0x48) succeeded on this kernel.

## Stats

| Category   | Count |
|------------|-------|
| Critical   | 0     |
| Important  | 2     |
| Suggestions| 5     |
| Positives  | 7     |

## Files in this review

- `00-overview.md` — this file
- `01-critical-and-important.md`
- `02-suggestions.md`
- `03-positive-notes.md`
- `04-action-items.md`
