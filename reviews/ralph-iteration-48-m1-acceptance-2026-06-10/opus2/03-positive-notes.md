# Positive Notes

- **The DHILOG ordering contract is genuinely safe here, and I verified the back-to-back
  OUT concern from the prompt is a non-issue.** The watermark check in `dhilog.rs::record`
  is `if self.record_count > 0 && icount < self.last_icount { return Err(IcountRegressed) }`
  — i.e. NON-DECREASING, equal icounts allowed, with `seq` providing total intra-icount
  order. So even though §3.1 says exiting instructions retire zero (two adjacent OUTs read
  the SAME icount via `counter.read()`), equal-icount appends are accepted and remain
  deterministically ordered by seq. There is exactly one multi-record dispatch in this run
  (the doorbell drain: CONS_BUMP then SDK_EVENT), and both share that exit's icount with
  deterministic seq order. The dedicated unit test `ordering_enforced_and_equal_icount_allowed`
  pins exactly this. Clean.

- **The dual-`GuestMem`-trait `VmMem` adapter over one Arc-backed `GuestMemoryMmap` is the
  right call** and is correctly reasoned in the doc comment: clones share the single mapping
  KVM runs against, so the channel's `M` handle and `DevCtx::mem` view identical bytes. No
  aliasing hazard because emulation is synchronous and single-threaded.

- **Base-image immutability is checked two independent ways** (blake3 of on-disk bytes AND
  mtime), and the hash is the real committed `BASE_IMAGE_BLAKE3` constant that a unit test
  in `image.rs` gates against generator drift. The CoW-overlay design guarantees the write
  cannot touch the `FileBase`, and this test proves it observationally. Strong.

- **`log_fault()` is checked after every single dispatch** in `on_exit`, and a fault is
  surfaced as a hard `BoundaryError::Exit` (DATA_LOSS-class, never absorbed) — matching the
  `DevCtx` contract precisely. The error-mapping for bus read/write and counter reads is
  also explicit and loud.

- **`StopReason::GuestHalted` is asserted**, not just relied upon — the test fails loudly if
  the guest ever stops for any other reason (budget, pause, goal). Good defensive posture.

- **The MachineConfig hash is plumbed end to end**: the same `config.config_hash()` feeds
  both the DHILOG `SegmentHeader.machine_config_hash` and the `StateHashChain` H_0, and the
  config carries the real `BASE_IMAGE_BLAKE3` — so a base-image change would perturb the
  state hash, tying the disk identity into the determinism chain.

- **x86-only gating is correct and verified live.** `#![cfg(target_arch = "x86_64")]`
  empties the whole file on arm; the new `[target.'cfg(target_arch="x86_64")'.dev-dependencies]`
  block keeps blake3/detguest-host/dh-devices/dh-inputlog off the arm build. `cargo check
  --target aarch64-unknown-linux-gnu` passes.

- **Repeatability is real, not flaky.** I ran the acceptance test 5 consecutive times; all
  green. The full workspace (including the 31s and ~100s live KVM suites) is green, clippy
  is clean, and the tree is clean.
