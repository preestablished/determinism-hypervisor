# Critical & Important Issues

## Critical

**None.** I ran the suite and three live experiments (see overview) and found no
determinism or correctness defect in the shipped diff. The IN-at-boundary path — the
genuinely new and highest-risk territory — replays bit-identically across runs at every
exact-icount target I tested.

## Important

### I1 — `classify_exit`'s `SerialIn`/`SerialOut` variants are a latent re-run of the iter-29 hazard for the future M1 loop (documentation)

**File:** `crates/dh-vmm/src/kvm.rs:265-268` (`ExitEvent::SerialIn { port, len }`),
cross-referenced with `tools/dh-cli/src/boot.rs:76-82` and `tools/dh-cli/src/run.rs:66-76`.

**Severity:** Important (documentation / future-integration trap — not a defect in this
diff's behavior).

The two debug loops wired in this iteration correctly intercept serial I/O on the **raw**
`VcpuExit::IoIn(port, data)` / `IoOut` exits *before* calling `classify_exit`, so they get
the live `&mut [u8]` and can call `DebugSerial::pio_read(port, data)` to fill the IN buffer
before re-entry. This is the only path that works, and it works.

But `classify_exit` *also* classifies the serial range as `ExitEvent::SerialIn { port, len }`
— carrying only the **length**, having dropped the mutable slice. Note the existing doc on
the sibling `DetcallIn` variant (kvm.rs:250-251): *"leaving it untouched hands the guest
stale bytes from a previous exit — host-visible nondeterminism."* A future M1 hashed run
loop built on `classify_exit` (the documented plan) that consumes `ExitEvent::SerialIn`
**physically cannot fill the buffer through that event** — the slice is gone. The guest
would resume with whatever KVM left in the shared kvm_run buffer: stale, host-visible,
nondeterministic. This is precisely the iter-29 class of bug the whole bead exists to
close, re-armed one layer up.

This is not introduced by this diff (the `SerialIn`/`DetcallIn` variants and the
"fill on raw exit" convention pre-date it, and no consumer of `SerialIn` exists yet —
verified by grep). But this iteration is the moment `DebugSerial` becomes the thing a
future caller will reach for, so the guard belongs here.

**Fix (pick one, documentation-level):**

Add a doc note on the `SerialIn` variant making the seam explicit, e.g.:

```rust
// crates/dh-vmm/src/kvm.rs, on ExitEvent::SerialIn
/// Serial RX (0x3F8..0x400). Like DetcallIn, the RAW exit's &mut buffer
/// MUST be filled BEFORE re-entry (DebugSerial::pio_read) — this event
/// carries only `len`, so a classify_exit-based loop CANNOT fill it here;
/// the run loop must intercept VcpuExit::IoIn on the raw exit (see
/// dh-cli boot.rs/run.rs). Resuming an unfilled serial IN hands the guest
/// stale kvm_run bytes — the iter-29 nondeterminism hazard.
SerialIn { port: u16, len: usize },
```

Optionally also add a one-line `// see ExitEvent::SerialIn doc: fill on the RAW exit`
near the M1 acceptance bead's run-loop entry point so the next implementer is warned
before they reach for `classify_exit`.
