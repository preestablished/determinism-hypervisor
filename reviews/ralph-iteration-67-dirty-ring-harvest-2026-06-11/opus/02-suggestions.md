# Suggestions (non-blocking)

### S1 — `harvest_at_boundary` skips reset when `harvested == 0` on a `DirtyRingFull` exit

- **Where:** `crates/dh-vmm/src/dirty.rs:193-199`
- The reset is gated on `harvested > 0`. That is the right optimization at a *pause boundary*
  (a clean ring needs no reset ioctl). But this same function is documented as the
  `KVM_EXIT_DIRTY_RING_FULL` service path (`dirty.rs:186-187`). On a genuine full exit there
  will always be entries, so in practice this never misfires. The only edge is a
  soft-full exit racing a drain that already happened on another path — not reachable in the
  single-vCPU/single-harvest-thread v1 design. Worth a one-line comment that the
  `harvested > 0` guard is a pause-boundary optimization and is always satisfied on a real
  ring-full exit, so the resume path stays loss-free. (Strictly informational; no behavior
  change needed for v1.)

### S2 — Document why a partial-RESET ring is still safe if `KVM_RESET_DIRTY_RINGS` fails mid-way

- **Where:** `dirty.rs:90-127` / `143-155`
- `harvest_into` marks each entry RESET as it goes, then `reset_dirty_rings` is a separate
  ioctl. If the ioctl errored, the entries are RESET-marked but not re-armed; the next
  `harvest_into` would correctly skip them (DIRTY bit clear) and the cursor has advanced — so
  no double-count and no loss, but the ring slots are stranded until a successful reset. In v1
  a failed `KVM_RESET_DIRTY_RINGS` is a hard error that aborts the run, so this is fine, but a
  sentence in the module header making the "RESET-marked but not yet reset-ioctl'd is a safe
  intermediate state" invariant explicit would help a future maintainer.

### S3 — `reset` count assertion (`stats.harvested == stats.reset`) is single-vCPU-specific

- **Where:** `dirty.rs:204-207` (doc comment) and `dirty.rs:381` (live assertion)
- `KVM_RESET_DIRTY_RINGS` is VM-wide and returns the count across *all* vCPU rings. The
  comment correctly notes "equals harvested in single-vCPU v1." Good. When multi-vCPU lands,
  this equality breaks (reset can exceed a single ring's harvest, or be zero if another thread
  already reset). The doc comment already flags it; just make sure the multi-vCPU bead inherits
  this caveat so the assertion at line 381 isn't copied forward blindly.

### S4 — `set_ram_flags` recomputes `userspace_addr` rather than reusing the slot's known value

- **Where:** `dirty.rs:165-183`
- It re-derives `userspace_addr` via `get_host_address(GuestAddress(0))`. `kvm.rs` has a
  `host_addr()` helper (kvm.rs:441-442) doing the same thing; consider reusing it for one
  source of truth on how the slot's base host address is computed (the two must stay identical
  or the flags-only re-registration would silently move the slot). Minor DRY.

### S5 — Magic `KVM_RESET_DIRTY_RINGS = 0xAEC7` could be derived rather than hardcoded

- **Where:** `dirty.rs:38-40`
- The constant is correct: `_IO(KVMIO=0xAE, 0xc7)` = `(0xAE << 8) | 0xc7` = `0xAEC7` (dir=0,
  size=0 ⇒ high bits zero). The comment documents the derivation well. As a hardening measure
  you could compute it with `nix::request_code_none!(0xAE, 0xc7)` or a small const fn so the
  encoding is checked by construction rather than by a hand comment — purely optional, the
  current form is fine and well-annotated.
