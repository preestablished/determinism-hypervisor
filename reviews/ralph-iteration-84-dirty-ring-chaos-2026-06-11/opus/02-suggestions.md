# Suggestions (non-blocking)

### S1 — Pin `page_dirtier`'s START_GPA and PAGES in `elf_shape.rs` (drift protection)

`tests/nanokernel/src/lib.rs` exports `PAGE_DIRTIER_START_GPA = 0x20_0000` and
`PAGE_DIRTIER_PAGES = 3072`, and the test uses `nanokernel::PAGE_DIRTIER_PAGES`. But the
asm `%define START_GPA 0x200000 / %define PAGES 3072` values are **not** pinned against
those Rust constants in `elf_shape.rs` — `page_dirtier` only gets the generic
`assert_guest_shape` call (elf_shape.rs:69), not a dedicated `%define`-vs-const test like
`pad_echo`, `entropy_draw`, `timer_guest`, and `landing_loop` all have.

This is precisely the drift the existing pins guard against: if someone edits the asm's
`PAGES` to 4096 but leaves `PAGE_DIRTIER_PAGES = 3072`, the `pages_shipped >=
PAGE_DIRTIER_PAGES` assertion still passes (it's `>=`, and 4096 ≥ 3072), silently weakening
the non-vacuity floor; or editing `START_GPA` could move the write region past the 16 MiB
slot with no compile-time catch. Add a `page_dirtier_asm_matches_rust_constants` test
mirroring the existing `%define`-lookup pattern, asserting `START_GPA == PAGE_DIRTIER_START_GPA`
and `PAGES == PAGE_DIRTIER_PAGES`. (Note: the constants are exported but currently
*unpinned*, so the lib values and asm values could already disagree undetected.)

### S2 — Document the fork inheriting the *default* ring size

`fork_slot_vm` hard-codes `DIRTY_RING_ENTRIES` (kvm.rs:209): a tier-A CoW child of a
custom-ring parent gets a 65536-entry ring regardless of the parent's size. For
production this is correct (forks are always default-ring). But it's an undocumented
asymmetry — the child's `dirty_ring_entries` will not match its parent's if the parent was
created via `create_slot_vm_with_ring`. Add a one-line comment at kvm.rs:209, e.g.
"// forks always use the production ring; a custom-ring parent (chaos tests only) does
not propagate its size — forks are never created from chaos slots." This pre-empts the
"why doesn't the fork inherit the parent's ring?" question.

### S3 — Tighten the non-vacuity floor: assert the *exact* expected overflow count

The test asserts `small.ring_full_exits >= 2`. With 3072 dirtied pages and a 1024-entry
ring at the kernel soft-full watermark, the expected number of ring-full exits is
deterministic-ish (≈ 3, modulo the watermark headroom and any BSS/page-table dirtying).
A `>= 2` floor is a reasonable lower bound, but consider also asserting an upper bound
(e.g. `<= 16`) so a regression that makes the watermark fire pathologically often (or a
mis-sized ring causing an exit per page) is caught. At minimum, the comment claims
"overflow the small ring 3 times" — the assertion only checks ≥2; align the prose and the
check, or note the watermark makes the exact count an inequality.

### S4 — Comment the magic `* 16` and `ring_entries * 16` with the struct size

kvm.rs:234 now reads `cap.args[0] = ring_entries * 16; // bytes of kvm_dirty_gfn` and
dirty.rs computes `map_len = entries * size_of::<kvm_dirty_gfn>()`. The kvm.rs side uses a
hard-coded `16` while dirty.rs uses `size_of`. These must agree (they do:
`sizeof(kvm_dirty_gfn) == 16`). Consider using
`ring_entries * std::mem::size_of::<kvm_dirty_gfn>() as u64` in kvm.rs too, so the two
sites can never disagree if the binding's struct ever changes — and so the existing
`DIRTY_RING_BYTES = DIRTY_RING_ENTRIES * 16` const (kvm.rs:21) doesn't drift from the new
per-ring computation. (`DIRTY_RING_BYTES` is now unused on the create path — verify it's
still referenced elsewhere or drop it.)

### S5 — Cross-link bead 0vl from the 16 MiB cap comment and add a regression guard idea

The 16 MiB `MEM` cap is well-commented in the test preamble and correctly filed as bead
0vl (P1, BUG). One addition: 0vl notes the 128 MiB perf acceptance (bead 9sb) will hit
this hang, but there's no `bd dep` linking 9sb's blocked-ness to 0vl. Consider
`bd dep add <9sb> determinism-hypervisor-0vl` so the perf acceptance cannot be picked up
before the store-hang is fixed. Also: a hang-as-failure is the worst mode — when 0vl is
worked, a client-side put timeout (loud error instead of `ep_poll` forever) would be worth
the test harness asserting against, so future regressions fail fast rather than wedge CI.
