# Suggestions (non-blocking)

### S1 — Padding is empirically zero today, but `encode_section` byte-copies it; add a regression guard

- **File:** `vcpu_state.rs:228-235` (`struct_bytes`) / `261-278` (`encode_section`).
- My live audit on kernel 6.8 found **every** reserved/padding range zero:
  segment `padding` bytes, dtable `padding[3]`, `kvm_fpu.pad1`/`pad2`,
  `kvm_xcrs.padding[16]` + the unused `xcrs[nr_xcrs..16]` entries,
  `kvm_vcpu_events.reserved[26]`, `kvm_debugregs.reserved[9]`. This is because
  KVM's `GET_*` ioctls zero them — unlike raw XRSTOR/XSAVE (the iter-69 class),
  so the byte-copy is safe **today**.
- It is *not contractually guaranteed* across kernels/CPUs that every reserved
  field stays zero forever (a future kernel could surface a bit in
  `events.reserved` or repurpose `dbg.flags`). Because `encode_section` feeds
  byte-equality and the cross-fork compare, a future nonzero reserved byte that
  is **instance-specific** would silently break fork/restore equality.
- Suggestion: either (a) zero the known padding/reserved ranges in
  `encode_section` before the byte-copy (belt-and-suspenders, mirrors the
  XSAVE canonicalization philosophy), or (b) add a live unit test that asserts
  those specific ranges are zero post-capture, so a kernel that starts
  surfacing them trips CI loudly instead of corrupting equality silently. I
  lean toward (b) as the cheaper guard given the empirical "KVM zeros them" fact.

### S2 — Module doc cites kvm-ioctls/kvm-bindings versions that don't match the lockfile

- **File:** `vcpu_state.rs:17,37,165` ("kvm-ioctls 0.24", "0.14 binding").
- The actual dependency in this tree is **kvm-bindings 0.13.0** (confirmed at
  `~/.cargo/.../kvm-bindings-0.13.0/`). The "0.14 binding" SAFETY note about
  "no FAM tail in the 0.14 binding" reasons about a version that isn't the one
  compiled. The conclusion (plain `kvm_xsave`, no FAM tail) happens to hold for
  0.13 too, but the citation is stale. Suggest correcting to the real versions
  so the SAFETY argument references the code that actually ships.

### S3 — The committed live test never runs the guest; fold in the post-execution fixed point

- **File:** `vcpu_state.rs:449-476` (`live_get_set_get_roundtrip`).
- The committed test perturbs `rax`/`rip` via `SET_REGS` but never calls
  `vcpu.run()`, so it proves the fixed point only for a never-entered vCPU. My
  scratch experiment ran the real-mode harness (`out 0xD3,al ; hlt`, two exits,
  rip advanced to 0x3) and the capture→restore→capture fixed point **still
  held** — which is the stronger and more reassuring property. Worth promoting a
  trimmed version of that experiment into the committed suite (it reuses the
  exact harness already in `kvm.rs:675`, so it's cheap and proven on this box).

### S4 — `restore` partially mutates the vCPU on a mid-sequence failure

- **File:** `vcpu_state.rs:143-191`.
- Restore issues SET_SREGS → SET_REGS → SET_FPU → SET_XCRS → SET_XSAVE →
  SET_DEBUGREGS → SET_VCPU_EVENTS → SET_MSRS → TSC offset, propagating the
  first error with `?`. A failure at, say, SET_MSRS leaves the vCPU in a
  half-restored state. For a determinism platform that's likely fine (a failed
  restore aborts the slot), but it's worth one sentence in the doc or the bead
  stating the failure contract: "a failed `restore` leaves the vCPU in an
  indeterminate state; the caller must discard the slot, not retry in place."
  No behavior change needed — just make the contract explicit so qmp/9e4 don't
  retry onto a partially-written vCPU.
