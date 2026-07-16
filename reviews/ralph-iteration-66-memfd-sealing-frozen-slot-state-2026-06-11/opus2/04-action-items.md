# Action Items

## Critical

_None._ No Critical findings. d2p is approvable as-is.

## Important

These are cross-bead spec reconciliations, not d2p code defects — the code's behavior is
correct in both cases. File against the fork epic / bead 9e4.

- [ ] **I-1 — Reconcile the `Faulted` slot state.** The proto
  (`.agents/docs/determinism-hypervisor/API.md:441-442`) defines `SlotState::FAULTED_S =
  5`, but the Rust `enum SlotState` (`crates/dh-vmm/src/lib.rs:33-41`) has only Empty /
  Running / Paused / Frozen. Today's `match` in `ensure_write_path` (lib.rs:104) is
  exhaustive, so adding `Faulted` later is compiler-forced — but the transition relation
  (`can_transition`, lib.rs:78-90) has **no `* → Faulted` edge**, so a guest-contract
  fault would have nowhere legal to land. In d2p: drop a `// TODO(fault-path): Faulted —
  see API.md SLOT::FAULTED_S` beside the enum. In the fault/fork bead: add the enum
  variant, add `Running → Faulted` / `Paused → Faulted` / `Faulted → Empty` edges, and a
  `FaultedWriteDenied { api }` arm.

- [ ] **I-2 — Fix the stale ARCHITECTURE lifecycle one-liner.** Line 740-741 reads
  `... Paused ⇄ Running → Frozen ...`, implying a direct `Running → Frozen`. The code
  (correctly, per §8.4 "once the parent pauses") rejects that and requires
  `Paused → Frozen` (lib.rs test `running_cannot_be_destroyed_or_frozen_directly`,
  lib.rs:233-237). Edit the doc line to `... Running, Paused → Frozen ...` or add
  "(parent must Pause first)" so the fork-bead author doesn't trust the wrong arrow.
  **Doc-only change; no code change.**

## Suggestions

- [ ] **S-1 — Generalize `ram_memfd()` beyond region 0.** `find_region(GuestAddress(0))`
  (kvm.rs:235) assumes a single flat RAM region. Either add a comment asserting that
  invariant, or (preferred for the fork consumer) seal **every** `file_offset()`-backed
  region so all of the parent's RAM is frozen, not just the one at PA 0.

- [ ] **S-2 — Defer, don't ossify, the `api: &'static str` choice.** Keep `&'static str`
  for now (zero-alloc, no production callers yet). When the fork beads add real call
  sites, if the write-path operation set stabilizes at a small fixed list, promote to a
  `WritePathApi` enum for exhaustiveness; otherwise keep the string. No change now.

- [ ] **S-3 — (optional) early-return `freeze_ram` if already sealed.** A
  `if self.ram_seals()? & WANT == WANT { return Ok(()); }` skips a redundant
  `F_ADD_SEALS` syscall on re-freeze. Purely cosmetic; current code is fine.

- [ ] **S-4 — (optional) factor the test's repeated `libc::mmap` into a local helper** to
  reduce `#[allow(unsafe_code)]` repetition and read as a prot/result table
  (kvm.rs:529-618). Readability only.

## Verification log (for the record)

- [x] `cargo test -p dh-vmm --lib` → 80 passed, 0 failed (incl. live `freeze_ram` test +
  5 slot-state tests).
- [x] `cargo clippy -p dh-vmm --lib` → clean.
- [x] Scratch C probe `/tmp/sealprobe.c` on kernel 6.8.0-124 → **`MAP_PRIVATE` CoW-writable
  mmap of a FUTURE_WRITE-sealed memfd SUCCEEDS**; shared-writable denied (EPERM); RO
  shared OK; mprotect-escalate denied (EACCES). **Bead 9e4 tier-A CoW fork design is
  confirmed sound — green light, no design change.**
- [x] Skeptic angles cleared: re-restore allocates fresh slot (no `Paused→Paused` needed);
  unfreeze-then-reuse is sound under irrevocable FUTURE-only seals; test isolation clean
  (per-test fresh memfd, no default seals).
