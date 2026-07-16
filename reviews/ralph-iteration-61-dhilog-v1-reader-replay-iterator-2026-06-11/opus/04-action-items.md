# Action Items

## Action Items

### Critical

_None._ The `LogReader::parse` decode path is total and panic-free over untrusted bytes;
all 35 tests pass and clippy is clean.

### Important

- [ ] **Harden `Record::body()`'s public contract** (`crates/dh-inputlog/src/reader.rs:111–178`).
  `Record` has all-`pub` fields and a `pub body()` documented as "Infallible," but a
  hand-constructed `Record { kind: KIND_END, payload: &[], .. }.body()` panics on
  `p[0]`/`p[8..40]` — the infallibility invariant lives in `parse()`, not the type.
  Preferred fix: make `Record`'s fields private and expose accessors so the only
  construction path is the validated iterator, making the "infallible views over
  already-validated bytes" claim true by construction. Alternatives: make `body()` total
  via `get(..)` + `Unknown`/`Malformed` fallback, or (cheapest) correct the doc-comment to
  state the parsed-record precondition. No in-tree caller hits this today; this closes a
  latent panic in a crate whose headline guarantee is totality.

### Suggestions

- [ ] **Update API.md §3.1 table** (`.agents/docs/determinism-hypervisor/API.md:520`):
  split the `240 | 16 | reserved` row into `240 | 8 | encoder_fingerprint` and
  `248 | 8 | reserved`, to match the writer (`dhilog.rs:299`, the in-repo authority) and
  this reader. Doc-only; the code split is correct. (S1)

- [ ] **Reject `clock_den == 0` at parse time** (`crates/dh-inputlog/src/reader.rs:354–369`)
  with a new `ReadError::BadClock`, so an untrusted log can't smuggle a divide-by-zero into
  a downstream replayer. Not mandated by §3.1, but consistent with the platform's
  validate-up-front posture. (S2)

- [ ] **Split or annotate `EndMismatch`** (`reader.rs:70–72, 427–435`) so the four distinct
  END violations (`boundary_rip != 0`, nonzero pad, `icount`, `end_state_hash`) are
  distinguishable by a forensic/divergence tool. (S3)

- [ ] **Add positive coverage for NET_RX / EPOCH_HASH and a golden-bytes fixture**
  (`crates/dh-inputlog/tests/reader_validation.rs`): a hand-rolled NET_RX at the 2048
  boundary (and 2049 rejected), an EPOCH_HASH log driving the `FLAG_EPOCH_HASHES` *true*
  path, and at least one pinned-bytes decode assertion so coordinated writer/reader layout
  drift is caught (round-trips alone can't catch wrong-but-symmetric layouts). (S4)

- [ ] **Extend `rejects_end_mismatch`** (`reader_validation.rs:407–422`) to also surgery
  the END `boundary_rip` nonzero and an END pad byte nonzero, covering all four
  `EndMismatch` causes. (S5)

- [ ] **Remove or comment the unreachable `_ => true` arm** in `validate_kind`'s layout
  match (`reader.rs:480–493`); by that point `kind` is always a known kind. (S6)
