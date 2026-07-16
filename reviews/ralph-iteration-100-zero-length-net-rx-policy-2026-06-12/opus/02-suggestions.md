# Suggestions (non-blocking)

These are optional polish items. None block the merge.

## S1 — Consider a one-line note in `golden.rs`'s "Deliberately absent" list

`crates/dh-inputlog/tests/golden.rs:19-22` enumerates what the freeze
deliberately does not cover. The new `1..=2048` lower bound on NET_RX is a
*reader-validation* tightening that lives in `reader_validation.rs`, not in the
golden freeze — and correctly so, since the kitchen-sink fixture's NET_RX is
non-empty (5 bytes) and unaffected. No change to `golden.rs` is *required*. But
a future reader auditing "why does the freeze accept NET_RX bytes but
validation has a lower bound?" would benefit from a one-line pointer. This is
purely a documentation nicety; the freeze itself is correct as-is.

## S2 — `MAX_NET_RX_FRAME` vs the literal `2048` in docs

The code uses the symbolic `MAX_NET_RX_FRAME` (good), while API.md and the
ledger use the literal `1–2048`. That's appropriate for prose docs, but if the
cap ever changes, three doc surfaces (API.md §3.3, ledger #19, the `net.rs`
`MAX_FRAME` const, and the `dhilog.rs` `MAX_NET_RX_FRAME` const) must move
together. No action now — just a maintenance note. The lower bound `1` is
hard-coded in `validate_kind` and the writer; that's fine since `1` is a
structural floor (non-empty), not a tunable.

## S3 — Comment wording in `net.rs`

The added comment (`net.rs:61-64`) says zero-length frames "land here as
`FrameTooBig`". This is accurate (`net.rs:158`: `len == 0 || ...` →
`FrameTooBig`), though "FrameTooBig" for a zero-length frame reads slightly
oddly on first encounter. The comment already explains the rationale well, so
this is a non-issue — noted only for completeness. Renaming the variant to
something like `BadFrameLen` would be a larger, out-of-scope change.
