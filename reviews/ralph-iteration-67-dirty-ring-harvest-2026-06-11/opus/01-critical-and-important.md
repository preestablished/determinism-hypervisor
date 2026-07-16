# Critical & Important Findings

## Critical

None. The unsafe KVM systems code is correct against the kernel ABI and QEMU's reference
implementation (details below, and in 03-positive-notes.md).

---

## Important

### I1 — ARCHITECTURE §8.2/§2.2 says `KVM_MEM_LOG_DIRTY_PAGES` is bitmap-only; the kernel requires it for the ring too. Route to bead `veu`.

- **Severity:** Important (documentation divergence — the *code is correct*, the doc is stale)
- **Where:**
  - Doc: `.agents/docs/determinism-hypervisor/ARCHITECTURE.md:118-119` ("`KVM_MEM_LOG_DIRTY_PAGES`
    is set on the RAM memslot **only on the bitmap fallback path**") and `:661-664`
    (same claim restated in §8.2).
  - Code: `crates/dh-vmm/src/dirty.rs:161-183` (`enable_dirty_logging` / `set_ram_flags`
    sets `KVM_MEM_LOG_DIRTY_PAGES` on the live memslot) and the module header at
    `dirty.rs:157-160` ("Without the flag the ring stays empty").

- **Description:** The doc asserts the memslot dirty-tracking flag is exclusive to the bitmap
  fallback. That is wrong at the kernel level. KVM only pushes `kvm_dirty_gfn` entries into the
  ring from the write-fault path that is gated by `kvm_slot_dirty_track_enabled()` — i.e. a
  memslot with `KVM_MEM_LOG_DIRTY_PAGES` set. With the flag clear the ring stays empty, exactly
  as the implementation comment says. (Confirmed against the upstream patch "KVM: Do not reset
  dirty GFNs in a memslot not enabling dirty tracking," and the QEMU `accel/kvm` convention of
  setting `KVM_MEM_LOG_DIRTY_PAGES` on tracked slots regardless of ring-vs-bitmap.) What *is*
  mutually exclusive is the **harvest mechanism** — with the ring enabled VM-wide,
  `KVM_GET_DIRTY_LOG` is refused (EINVAL); the *memslot flag* is shared by both paths. So the
  doc's "the ring and the dirty bitmap are mutually exclusive per VM" half is right; the
  "flag only on the bitmap path" half is wrong.

  The live test only ever runs *with* the flag, so it cannot by itself prove the flag is
  necessary. But the kernel ABI settles it: the flag is required for the ring to publish, and
  the implementation correctly sets it. **No code change is warranted** — this is purely the
  ARCH wording contradicting the (correct) implementation.

- **Fix:** No code change. Add a divergence entry to bead `veu` (the divergence collector,
  which already carries Divergence #4) and correct the two ARCHITECTURE lines upstream. The
  paragraph below is suitable for the bead note and the doc fix:

  > Divergence #5 (iteration 67, bead ygt): ARCHITECTURE §2.2 (line 118) and §8.2 (lines
  > 661-664) state `KVM_MEM_LOG_DIRTY_PAGES` is set on the RAM memslot "only on the bitmap
  > fallback path." This is incorrect: KVM publishes dirty-ring entries only for memslots with
  > dirty tracking enabled, so the *ring* path also needs the flag. `dirty::enable_dirty_logging`
  > sets it for the ring; code is authoritative. The genuinely-exclusive part — ring enabled
  > VM-wide forbids `KVM_GET_DIRTY_LOG` — should be retained; only the "flag is bitmap-only"
  > claim is wrong. Suggested rewording: "`KVM_MEM_LOG_DIRTY_PAGES` is set on the RAM memslot
  > on **both** the ring and bitmap paths (the kernel publishes ring entries only for
  > dirty-tracked slots); the ring and the bitmap *harvest* mechanisms are mutually exclusive
  > per VM (enabling the ring forbids `KVM_GET_DIRTY_LOG`)."
