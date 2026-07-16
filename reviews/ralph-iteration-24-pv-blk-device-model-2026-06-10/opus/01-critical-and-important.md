# Critical and Important Findings

## Critical

None.

---

## Important

### I-1 — Partial-completion on `STATUS_MEM_FAULT` mutates guest RAM and overlay; replay-safety is asserted but neither enforced nor tested

`blk.rs:146-205`. Both `do_read` and `do_write` operate chunk-by-chunk and can
return `STATUS_MEM_FAULT` *after* one or more chunks have already landed:

- `do_read` (`blk.rs:162`) writes each chunk into guest RAM as it goes; a fault
  on chunk N leaves chunks `0..N` already written to guest memory.
- `do_write` (`blk.rs:183-198`) populates (RMW-from-base) and fills overlay
  clusters as it goes; a fault while copying from guest RAM into cluster N
  leaves clusters `0..N` fully populated **and** cluster N populated-from-base
  but with `[within..within+take]` unmodified (the source read faulted), while
  the device's overlay now permanently contains those clusters.

The inline comment at `blk.rs:196-197` claims this is "a deterministic function
of the same failing request." That is true **only** under an assumption the
code does not state or guarantee: that on replay the *same* request registers
(SECTOR/BUF_GPA/COUNT), the *same* base content, the *same* prior overlay state,
and the *same* guest-RAM contents are presented, and that the guest-memory
backend faults at the *exact same byte offset*. The first three are part of the
deterministic state; the fourth (where `ctx.mem` decides to fault) is a property
of the real guest-memory implementation, which is not the test `VecGuestMem`.

Why this matters: the device leaves observable side effects (guest RAM bytes +
overlay clusters) on a path that returns a *fault* status. For replay
equivalence, the partial side effects must be a pure function of replayable
state. With `VecGuestMem` the fault boundary is deterministic (an all-or-nothing
slice bound at `ctx.rs:152/160`) — but note `VecGuestMem::write` is itself
all-or-nothing, so in the *current* test backend a write either fully lands or
writes nothing, meaning the "partial overlay then fault" scenario for `do_write`
where the *guest read* faults mid-cluster is only reachable across cluster
boundaries, not within one `ctx.mem.read`. The real `GuestMemoryMmap` backend
(referenced in `ctx.rs:145`) may fault at a sub-slice granularity. Until that
backend lands, the partial-mutation determinism is unproven against the
production memory model.

Recommendation:
1. Add a normative doc comment on `do_read`/`do_write` stating the
   partial-completion contract explicitly: "On `MEM_FAULT`/`HOST_IO`, guest RAM
   and overlay are left in a partially-applied state that is a pure function of
   {registers, base, overlay, guest RAM}; replay reproduces it iff the
   guest-memory backend faults deterministically at the same offset." Make this
   a stated invariant the future `GuestMemoryMmap` backend must satisfy.
2. Add a test that drives a multi-cluster write/read which faults on a *middle*
   chunk and asserts the exact set of clusters populated and bytes written, so
   the partial-application surface is pinned. (See test gap T-1 in 02.)
3. Consider whether the cleaner contract is "validate the entire BUF_GPA range
   against guest memory up front, before mutating anything" so that
   `MEM_FAULT` is all-or-nothing. That removes the partial-mutation reasoning
   entirely and is closer to how a hardware DMA engine reports a fault without
   committing a partial transfer. This is the strongest fix and worth
   discussing — see also NEEDS_DISCUSSION note in 02.

### I-2 — `host_io_errors` slot-fatal handling is a documented convention, not an enforced contract

`blk.rs:49-55, 88-90, 116-126`. `STATUS_HOST_IO` (0xFE) is guest-visible by
design, and the doc correctly explains that a host I/O error cannot be
guaranteed to reproduce on replay, so run control must treat a post-dispatch
nonzero `host_io_errors` as slot-fatal. This is the right model. But:

- The branch adds the counter and the doc, yet there is no caller in this diff
  that actually reads `host_io_errors` after dispatch and faults the slot. The
  contract lives entirely in a doc comment. If the dispatch integration (run
  control) is in a later iteration, that is fine — but the device is now
  *capable* of silently returning 0xFE to a guest with nothing wired to detect
  it. A guest that ignores STATUS would proceed on partially-read (zero-filled
  on the faulting chunk? no — `do_read` returns before writing the faulting
  chunk, so guest RAM has the *prior* chunks only) data, and replay could
  diverge undetected until/unless the slot is faulted.

Recommendation: file a bead (or confirm an existing one) for "run control must
check `PvBlk::host_io_errors` after each dispatch and fault the slot on
increase," and add an assertion-style integration test once the dispatch seam
exists. At minimum, cross-reference that bead from the doc comment so the
contract is traceable rather than aspirational.

### I-3 — `do_read` returns `MEM_FAULT` without zero-filling or marking the partial guest buffer; a non-checking guest sees stale RAM

`blk.rs:162-164`. On a read fault, chunks `0..N` are written from base/overlay,
chunk N onward are left as whatever the guest RAM previously held. The status is
the only signal. This is internally consistent and deterministic, but it is a
sharp edge: a guest-SDK bug that issues the read, ignores STATUS, and consumes
the buffer gets a mix of fresh and stale bytes with no poisoning. virtio
real-hardware semantics also leave the buffer undefined on error, so this is
defensible — but given the determinism mandate, consider documenting that the
buffer contents on a non-OK status are explicitly undefined-but-deterministic,
and that the guest contract is "STATUS != OK ⇒ buffer must not be consumed."
This pairs with I-1's "validate up front" option, which would make read faults
all-or-nothing too.
