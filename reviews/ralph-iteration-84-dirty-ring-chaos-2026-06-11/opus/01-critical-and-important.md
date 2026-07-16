# Critical & Important findings

## Critical

None.

The two correctness questions that could have been Critical both check out:

- **Cursor mask soundness across wraps.** `harvest_into` indexes with
  `(self.next_harvest % self.entries)` and `next_harvest` only ever increments
  (`self.next_harvest += 1`, never rewound). For `entries = 1024` and 3072 dirtied
  pages, the cursor sweeps slots 0..1023 three times; each slot is harvested (DIRTY→read→
  RESET) before the kernel can re-arm it, because the vCPU is exit-blocked on ring-full
  until `KVM_RESET_DIRTY_RINGS` frees slots. No entry is read twice and none is skipped.
  Sound.

- **`harvest_at_boundary` on a FULL ring is the documented loss-free path.** Its own
  doc comment (dirty.rs:204–205) states it is "Also the loss-free
  `KVM_EXIT_DIRTY_RING_FULL` service path — same call, then re-enter the guest," and the
  module preamble (dirty.rs:9–14) explains why: KVM exits at a soft-full watermark with
  headroom and cannot re-enter until reset re-arms slots, so a harvest+reset on the
  full-exit drains everything published so far. `classify_exit` maps
  `KVM_EXIT_DIRTY_RING_FULL` → `ExitEvent::DirtyRingFull` (kvm.rs:545–547), and the test
  loop services exactly that. Correct.

---

## Important

### I1 — Doc/bead trail still says "512"/"65536"; the 1024 floor is recorded only in the test

The bead `determinism-hypervisor-28i` title and description both say **"ring size 512"**.
`.agents/docs/determinism-hypervisor/IMPLEMENTATION-PLAN.md:79` says
*"Dirty-ring-full forced (ring size 512) — hashes unchanged vs large ring."*
`ARCHITECTURE.md:656` describes the ring as "ring size 65536 entries" and §8.2 promises
the property is "verified by a dedicated determinism test that forces tiny rings"
(pre-existing text — this branch did not touch the docs at all; `git diff main...HEAD
--stat` shows zero doc files).

The landed test uses `SMALL_RING = 1024` because — per the test's own doc comment — the
kernel reserves 64 + 512 (PML) ring entries on x86 and rejects rings below that floor
with EINVAL, making 1024 the smallest legal power-of-two ring. That empirical is real and
correctly handled, but it lives **only** in the source comment of `ring_chaos.rs`. The
canonical planning docs and the tracking bead are now stale/contradictory.

Why it matters: the next reader who greps the plan/bead for "512" and the code for "1024"
sees a mismatch with no breadcrumb tying them together, and the §8.2 sentence "forces
tiny rings" is now backed by a test whose "tiny" is 16× the planned size. This is exactly
the kind of drift the project's own `elf_shape.rs` drift-pins exist to prevent.

**Action:** Update IMPLEMENTATION-PLAN.md:79 and the bead 28i description to "ring size
1024 (smallest the kernel accepts: 64 + 512-PML reserved entries floor; the plan's 512 is
below the EINVAL floor)". Optionally add a one-line note in ARCH §8.2. Closing 28i should
record the 512→1024 reason in the close message.

### I2 — `DirtyRing::map`/`map_sized` takes no `&SlotVm`; ring-size consistency is unenforced

`SlotVm::dirty_ring_entries` records the size the VM was created with, and the field's
doc (kvm.rs:330–332) correctly warns "a mismatch would mis-mask the free-running cursor."
But `DirtyRing::map_sized(vcpu, entries)` takes a raw `&VcpuFd` and a separately-supplied
`entries`; nothing forces `entries == slot.dirty_ring_entries`. `DirtyRing::map(vcpu)`
silently uses the 65536 default.

Today this is latent, not live: every existing `DirtyRing::map(...)` caller
(`snapshot_engine.rs:158`, `store_durability.rs:119`, `restore_engine.rs:292`,
`dirty.rs:336/354`) operates on default-ring slots, and the chaos test correctly uses
`map_sized(&slot.vcpu, ring_entries)` with the same value it passed to
`create_slot_vm_with_ring`. So there is no current bug.

The hazard is for the *next* author: a `map(&custom_slot.vcpu)` would mmap 65536 entries
over a 1024-entry kernel ring (over-large mmap of a too-short ring → reads of unmapped/
zeroed slots past the real ring, and a cursor mask 64× too wide), or a `map_sized` with a
typo'd size mis-masks. None of this is caught at compile time, and the symptom (silently
dropped dirty pages) is exactly the failure this test exists to rule out.

**Action (pick one):**
- Preferred: add `DirtyRing::map_for_slot(slot: &SlotVm) -> Result<Self,_>` that calls
  `map_sized(&slot.vcpu, slot.dirty_ring_entries)`, and have all callers (incl. the chaos
  test) use it. `map`/`map_sized` can stay for the rare raw case.
- Or at minimum: a `debug_assert`/doc-warning on `map_sized` that `entries` must equal the
  slot's `dirty_ring_entries`, and a sentence on `map` that it is **only** valid for
  default-ring slots.

### I3 — Acceptance shape diverges from the bead's "roundtrip / H1==H2"; document the substitution

The bead frames the property as a *roundtrip* whose snapshot **hashes must equal** the
65536-ring run, echoing the M4 H1==H2 transparency shape. The landed test instead asserts
**delta-ref equality + pages_shipped equality + bit-equal vCPU capture**, with no
restore-and-replay leg.

This is, on inspection, an **equal-or-stronger** discharge, and that is worth stating
plainly so reviewers don't read it as a weakening:
- `SnapshotRef` is `BLAKE3(manifest_body)` — a content digest, not a server-assigned id.
- The incremental path (`snapshot_engine.rs`) folds the **DHSNAP device blob** (which
  carries the captured vCPU/device state) into the manifest body alongside the
  per-page `(index, BLAKE3(page))` entry table.
- Therefore `small.delta.snapshot_ref == large.delta.snapshot_ref` already implies
  identical page content, identical page indices, **and** identical vCPU/device bytes.
  A single lost or mis-ordered dirty page, or any vCPU perturbation from the extra exits,
  changes the ref. The separate `assert_eq!(small.vcpu, large.vcpu)` is therefore
  redundant with the ref check but harmless (and useful as a failure *localizer* — it
  tells you *whether* a ref mismatch came from vCPU state vs pages).

What ref-equality does **not** cover that an H1==H2 roundtrip would: that the snapshot is
*restorable* and that a restored slot *replays identically*. R8 is specifically "ring-full
loses no dirty page," which ref-equality discharges directly and arguably better than a
hash-chain (it pins per-page content + index, not just an aggregate). So the substitution
is defensible.

**Action:** Add 2–3 sentences to the bead 28i close note (or the test preamble) stating
explicitly: *the discharge is delta-ref content-equality, which is ≥ the planned
hash-equality because the ref covers page content+indices+DHSNAP/vCPU; a restore-replay
leg is intentionally out of scope (covered by 7c8's H1==H2) and R8 is about page loss, not
restorability.* If the project wants the literal roundtrip too, file a follow-up to add a
restore-and-compare leg — but I would not block on it.
