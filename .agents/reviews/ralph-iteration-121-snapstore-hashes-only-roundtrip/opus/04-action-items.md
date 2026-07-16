Required:

- None.

Recommended:

- Add a live missing-parent case if dh-snapshot relies on that arm of the typed missing-pages contract. `crates/dh-snapshot/tests/snapstore_readiness.rs:413-420` covers `ClientError::MissingPages` with `parent_ref: None`; a delta manifest with an unknown parent would pin `parent_ref: Some(_)` and empty `page_hashes`.

Optional:

- If missing-page hash order is meant to be part of the contract, document that near `crates/dh-snapshot/tests/snapstore_readiness.rs:420`. If only completeness matters, compare as sets to avoid over-pinning response order.
