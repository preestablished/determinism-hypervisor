# Action Items

## Critical

- [ ] None.

## Important

- [ ] None blocking merge. (The ARCHITECTURE §8.2 / §2.2 "`KVM_MEM_LOG_DIRTY_PAGES` only on
      the bitmap path" wording is wrong — I confirmed by experiment that the flag is required
      for ring publication on kernel 6.8 — but the **code is correct** and this doc fix is
      already raised by reviewer 1 and tracked in bead `veu`. No action duplicated here.)

## Suggestions

- [ ] **S1 — Single-source `PAGE_SIZE`.** Two public `PAGE_SIZE` constants in dh-vmm with
      different types: `dirty.rs:25` (`pub const PAGE_SIZE: u64 = 4096`) and
      `hash.rs:33` (`pub const PAGE_SIZE: usize = 4096`). Define one canonical constant
      (e.g. beside `DIRTY_RING_ENTRIES` in `kvm.rs`) and have both reference it; if the two
      types are both needed, derive one from the other so there is a single source of truth.
      Non-blocking; values agree today.

- [ ] **S2 — Document the mid-harvest-error abort semantics.** Add a one-line note on
      `harvest_into` / `harvest_at_boundary` (`dirty.rs:84-121`, `:184-199`): an error
      mid-drain is a hard contract violation; some entries may already be RESET-marked but
      unreaped (harmless — a later VM-wide `KVM_RESET_DIRTY_RINGS` reaps them, confirmed by
      experiment), and the caller (snapshot engine, bead `qmp`) must treat the boundary as
      fatal, NOT retry it. Pre-empts a wrong "retry the boundary" loop downstream.

- [ ] **S3 — Tighten soft-full wording.** In the module header (`dirty.rs:11-12`), note that
      the ring-full exit fires at a soft-full watermark (kernel reserves headroom) rather than
      at a physically-full ring. The loss-free guarantee is unaffected — soft-full only
      strengthens it — this is a precision nudge.

- [ ] **S4 — (No action; verified.)** The "forced tiny-ring determinism test" promised by
      ARCH §8.2 is already filed as bead **28i** (M4-accept: ring size 512 forced, hashes
      unchanged) and bead **v1n** (nightly chaos: tiny dirty rings). No new bead needed.
      Optionally cross-reference 28i from the §8.2 comment in `dirty.rs` so the
      "where's the forced-full test?" question is self-answering. Non-blocking.
