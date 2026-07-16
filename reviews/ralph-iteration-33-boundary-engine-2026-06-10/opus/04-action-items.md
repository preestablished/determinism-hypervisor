# Action items

Self-contained tasks distilled from this review. None block merge of this
foundation slice; all should be tracked as beads before §3.3 run-control wires
`land_at` to a live config.

### Critical

None.

### Important

- **A-1: Wire `MachineConfig` margins into the boundary engine's `Margins`.**
  `crates/dh-vmm/src/boundary.rs:35-49` defines `Margins { skid_margin: u64,
  resync_slack: u64 }` with defaults 8192/1024; `crates/dh-vmm/src/config.rs`
  defines `MachineConfig.skid_margin: u32` / `resync_slack: u32` with the same
  `DEFAULT_SKID_MARGIN`/`DEFAULT_RESYNC_SLACK` constants. There is no conversion
  and no non-test caller of `land_at`. Add `impl From<&MachineConfig> for
  Margins` (widening u32→u64) and make it the ONLY way run-control builds a
  `Margins`. Add a test asserting the two default constants agree. Until wired,
  add a doc note on `Margins` pointing at `MachineConfig::skid_margin` as the
  operator-facing source. Risk if skipped: a host with a tuned `skid_margin`
  silently runs the hardcoded 8192 → latent Overshoot (R1). File as a bead,
  blocked-by nothing, blocking the §3.3 run-control bead.

### Suggestions

- **A-2:** Document on `land_at` that a final `set_singlestep(false)` failure
  supersedes the loop result (incl. a successful landing) — we never return a
  boundary the caller would resume from a vCPU still in SINGLESTEP (R10).
  `boundary.rs:163-169`. (See 02-suggestions.md S-1.)

- **A-3:** Add a one-line comment (`counter.rs:140` or `boundary.rs:128`)
  stating that `PERF_EVENT_IOC_PERIOD` takes effect from the current count
  (kernel `perf_event_period`), and that the exact-N landing tests are the
  empirical proof. (S-2.)

- **A-4:** Document that non-debug exits encountered mid-step (notably
  `VcpuExit::Hlt`) are delivered to `on_exit`; the engine does not special-case
  HLT. Landing across a HLT is the callback's responsibility. (S-3.)

- **A-5:** Future live test — land across a guest doing MMIO/PIO with an
  `on_exit` that services them, asserting exact landing despite interleaved
  exits (the only untested branch of the engine's contract). Gate on the device
  run loop landing. (S-4.)

- **A-6:** Future live test — REP-instruction landing (target inside a `REP
  MOVSB`/`STOSB`): assert exact N, `rip` == REP RIP, `rcx` mid-progress, and no
  boundary declared with RIP unchanged across a step. Uses the bead-d34 REP
  guest. (S-5.)

- **A-7:** Note in the bead that boundary diagnostics are O(1) per landing (one
  `KVM_GET_REGS` at the boundary, `boundary.rs:116`) — keep `get_regs` out of
  the step loop. (S-6.)
