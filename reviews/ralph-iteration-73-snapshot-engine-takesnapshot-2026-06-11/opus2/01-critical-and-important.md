# Critical & Important Findings

**None.**

No Critical and no Important findings. The byte-determinism property that
everything downstream rests on was verified empirically in both the
same-slot and the harder cross-VM case (see `00-overview.md` experiments
1–2): two independently built slots+buses with identical seed/config
produce the **same snapshot ref**, so no engine section leaks
nondeterministic bytes (timestamps, pointers, uninitialised padding, KVM
reserved fields, bus-iteration order). The §8.2 ordering invariants that
matter for correctness all hold:

- **Ref-after-durability**: the ref is returned only from
  `put_snapshot_from_parts`' success path; on any `Store(_)` error nothing
  is cleared (`take_snapshot` steps 5→6).
- **Post-ack dirty clear**: `dirty.clear()` runs *after* the
  `put_snapshot_from_parts` ack, as the last step — verified by the
  incremental test asserting `dirty.is_empty()` only post-success.
- **Preconditions gate before any store contact**: `AgendaNotEmpty` /
  `NotPaused` return before `put_pages`, confirmed by
  `preconditions_fail_loudly_without_touching_the_store`.
- **Canonical §4 section order is engine-fixed**, not bus-order-dependent:
  device sections are sorted by `KNOWN_TAGS` position, so a `PvBlk`
  registered out of base order still lands between `PADD` and `SERL`
  (experiment 3).

The one concrete improvement (double page upload) is an efficiency matter,
not a correctness one — the store dedups, the ref is identical — so it is
filed as a Suggestion in `02-suggestions.md`, not here.
