# Positive Notes

## Clear-after-ack ordering is exactly right, and the retry-safety reasoning holds in the code

`snapshot_engine.rs:246-249` clears the dirty set strictly after `put_snapshot_from_parts`
returns. I traced the retry path against the actual code and it is sound:

- `harvest_at_boundary` accumulates into the persistent `DirtyPageSet` (it does not consume
  it); `dirty.iter()` reads the bitmap; `clear()` zeroes it only at the very end.
- On `put_pages`/`put_snapshot` failure the `?` returns early — the set is untouched.
- A **retry** calls `take_snapshot` again with the same `&mut DirtyPageSet`. Re-harvest finds
  0 new entries (ring already drained, `reset_dirty_rings` already ran), `dirty.iter()`
  re-yields the same indices, pages re-read and re-shipped (idempotent: `put_pages` dedups
  server-side, see `client.rs:95`), and the manifest rebuilds identically.

This is the correct shape for an at-least-once orchestrator over an idempotent store. The
orphaned-pages-on-`put_snapshot`-failure case is correctly punted to GC (pages without a
referencing container), and the dirty-set-not-cleared case is correctly recoverable. The
module doc spells this reasoning out, which I appreciate.

## DHSNAP order decoupled from bus order — the right architectural instinct

`build_dhsnap` does NOT trust bus iteration order for the snapshot-ref preimage. Devices are
collected, then `sort_by_key` on `KNOWN_TAGS` position (`snapshot_engine.rs:347-352`). This
is the correct call: it makes the ref a function of *logical state*, not *registration
accident*, which is precisely what a content-addressed determinism platform needs. The
fixed-order section emission (MCFG/VCPU/LAPC/TIME/ENTR up front, devices sorted after) is
laid out explicitly rather than left to iteration. Good discipline.

## The entropy-device special case is handled and *enforced*

The 0x0004 fold-into-ENTR-v2 (the resolved 6yl landmine) is handled correctly: the device's
reg blob is captured during the bus walk, skipped from standalone framing, and combined with
`DetEntropy::state()` into `EntrSectionV2`. Crucially, a **missing** entropy device is a loud
`Codec` error (`snapshot_engine.rs:329-330`), not a silent ENTR-v1 fallback — and there's a
dedicated test (`missing_entropy_device_is_a_loud_codec_error`) proving it. This is exactly
the "missing = loud error" §4 discipline.

## Hash-vs-section reconciliation documented at the decision point

The module header (`snapshot_engine.rs:76-94`) records the iteration-70 option-(b) decision —
hash keeps `canonical_vcpu_blob`, section keeps `encode_section` — with the *reasoning* (not
re-importing the reserved-byte hazard class iteration 69 eliminated, no verification gain) and
a pointer to veu #8 for the stale ARCH wording. Decisions captured where the reader needs
them, with a tracked follow-up for the doc drift. This is how to leave a divergence.

## Tests use the REAL store in-process (R12 fidelity), not a mock

`spawn_store_blocking` stands up the actual `snapstore-server` via `serve_for_tests` over a
UDS with a readiness probe, and the engine reaches it through the production blocking facade.
The FULL test pulls the container back, decodes the manifest, parses the DHSNAP, and verifies
sections byte-for-byte (MCFG canonical encoding, TIME boundary, ENTR seed, VCPU decode ==
fresh capture, LAPC empty). The incremental test proves exactly-the-dirty-pages shipping,
the `parent` link in the DELTA manifest, and clear-after-ack. This is real joint testing, not
a stub — the right rigor for the platform's central operation.

## Precondition failures touch nothing

`preconditions_fail_loudly_without_touching_the_store` asserts both gates (agenda + all
non-Paused states) return errors before any store interaction. The gates are the first thing
`take_snapshot` does (lines 183-188), so a failed precondition cannot leak pages or partial
containers. Clean fail-fast.
