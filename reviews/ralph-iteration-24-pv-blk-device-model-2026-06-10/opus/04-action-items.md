# Action Items

Each item is self-contained. File numbers reference 01/02.

### Critical

_None._

### Important

- [ ] **(I-1) Document and test partial-completion semantics on fault.**
  `crates/dh-devices/src/blk.rs` `do_read` (146-170) and `do_write` (172-205)
  can return `STATUS_MEM_FAULT`/`STATUS_HOST_IO` after some chunks/clusters have
  already mutated guest RAM and the overlay. Add a normative doc comment stating
  the partial-application contract and that replay equivalence requires the
  guest-memory backend to fault deterministically at the same offset. Add a test
  that faults on a *middle* chunk of a multi-cluster request and pins the exact
  set of populated clusters and written guest bytes.

- [ ] **(I-2) Wire or track the `host_io_errors` slot-fatal contract.**
  `crates/dh-devices/src/blk.rs:88-126`. Nothing in this diff reads
  `PvBlk::host_io_errors` after dispatch. File/confirm a bead requiring run
  control to fault the slot on a post-dispatch increase of `host_io_errors`, and
  cross-reference it from the doc comment at `blk.rs:49-55` so the contract is
  traceable. Add an integration assertion once the dispatch seam exists.

- [ ] **(I-3) Document the buffer-on-error contract.**
  `crates/dh-devices/src/blk.rs:146-170`. A read that returns a non-OK status
  leaves the guest buffer partially fresh / partially stale with no poisoning.
  Document that on `STATUS != OK` the buffer is undefined-but-deterministic and
  must not be consumed by the guest.

### Suggestions

- [ ] **(D-1 / S-1) Decide on all-or-nothing fault semantics.** Consider
  pre-validating the full BUF_GPA range before mutating any state so
  `MEM_FAULT` becomes all-or-nothing, removing the partial-application replay
  reasoning entirely. Capture the decision in a design note / bead. Add the
  one-line "registers are last-completed-request latches; device is always
  quiescent at a snapshot boundary" comment near `snapshot`/`restore`
  (`blk.rs:243-291`).

- [ ] **(S-2) Note base-identity assumption in `restore`.**
  `crates/dh-devices/src/blk.rs:260-291`. `restore` accepts any cluster idx and
  any register values; base identity is enforced by the MachineConfig hash, not
  here. Add a doc note (and optionally a debug-assert that cluster idxs are
  within the current base's cluster count).

- [ ] **(S-4) Comment the cached-`len` / mid-run-truncation behavior in
  `FileBase::read_at`.** `crates/dh-vmm/src/blkfile.rs:45-57`. Note that the
  `open()`-time `len` is trusted and a racing truncation surfaces as
  `read_exact_at` → `BaseIoError` → `STATUS_HOST_IO`, the intended slot-fatal
  path.

- [ ] **(S-5) Add a reusable `BlockBase` zero-fill-past-EOF conformance test.**
  The fill-zero-then-copy-tail contract is duplicated in `VecBase`
  (`blk.rs:311-319`) and `FileBase` (`blkfile.rs:45-57`). A shared
  `#[cfg(test)]` conformance helper (offset past EOF → all zeros; partial tail →
  remainder zeroed) protects future backends.

- [ ] **(S-3, S-6) Minor doc polish.** Note why `0xFE` was chosen for
  `STATUS_HOST_IO`; record that the last cluster's overlay holds zero-filled
  bytes past the image tail which are non-addressable (no action needed beyond
  the comment).

### Test gaps to consider (from 02)

- [ ] **(T-1)** Mid-request fault on a multi-cluster span (supports I-1).
- [ ] **(T-2)** Read of a fully-overlaid multi-cluster span (current tests read
  at most across one base/overlay boundary; a span entirely in the overlay
  across ≥2 clusters is not directly asserted).
- [ ] **(T-3)** `restore`-then-`snapshot` idempotence (snapshot bytes equal
  after a round-trip) to pin the serialization is a fixpoint.
- [ ] **(T-4)** Write straddling the EOF cluster boundary on `FileBase` (write
  the last partial cluster, confirm zero-fill tail and base bytes unchanged).
