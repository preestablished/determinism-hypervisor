# Action Items

Branch: `ralph/iteration-100-zero-length-net-rx-policy` — Reviewer: Claude Opus
— 2026-06-12. Verdict: **APPROVE**.

### Critical

_None._

### Important

_None._

### Suggestions (optional, non-blocking)

- [ ] **S1** — Optionally add a one-line pointer in `golden.rs`'s "Deliberately
  absent from the freeze" comment (`crates/dh-inputlog/tests/golden.rs:19-22`)
  noting that NET_RX now has a `1..=2048` lower bound enforced by reader
  validation (`reader_validation.rs`), not the byte freeze. Documentation only;
  the freeze itself is correct and untouched.
- [ ] **S2** — Maintenance note only: if `MAX_NET_RX_FRAME` / `MAX_FRAME` ever
  changes, update the `2048` literal in `API.md` §3.3 and ledger #19 in lockstep
  with the `net.rs` `MAX_FRAME` and `dhilog.rs` `MAX_NET_RX_FRAME` consts. No
  change now.
- [ ] **S3** — Out of scope: the `NetRxError::FrameTooBig` variant is reused for
  the `len == 0` case (`net.rs:158`), which reads slightly oddly. The new
  comment already explains it; renaming would be a larger separate change. No
  action this iteration.

### Merge readiness

- [x] Full workspace builds clean (`cargo build --workspace`).
- [x] `dh-inputlog` golden, reader_validation, and lib test suites pass.
- [x] Golden v1 fixtures and BLAKE3 pins unchanged (no format-version bump
  needed).
- [x] Ledger entry #19 format and verbatim `Old` quote verified.
- [x] No exhaustive `WriteError` match breaks; `PartialEq`/`Eq` present for the
  `==` test assertions.

**Ready to merge as-is.** The suggestions above are optional and can be deferred
or dropped.
