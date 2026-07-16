# Positive notes

- **The unsafe-free boundary is fixed the right way.** The old
  `gettid() = std::process::id() as i32` in `tools/dh-cli/src/run.rs` was a latent
  correctness bug — pid == tid only on the main thread, so a worker-thread caller
  would silently route PMI kicks to the wrong thread. It's replaced with
  `dh_vmm::run::current_tid()`, which encapsulates the real `SYS_gettid` syscall
  with a SAFETY note in the crate that's allowed unsafe, keeping dh-cli's
  `#![forbid(unsafe_code)]` intact. Grep confirms no `process::id()`-as-tid
  remains anywhere. The doc comment on `current_tid()` even explains the pid/tid
  hazard for the next reader. Exactly the right factoring.

- **The gate semantics are airtight.** `assert_margin` is strict (`max < margin/2`,
  not `<=`), and an empty histogram **fails** ("no data is not a pass") — both
  pinned by unit tests (`margin_gate_is_strict_and_empty_fails`), and I confirmed
  the empty-fail path live via `--samples 0`. This is the correct conservative
  posture for an R1 alert.

- **Determinism of the measurement itself.** `dh-cli skid` (200 samples) returned
  bit-identical output (sum=5866) on all 5 runs, on a busy CI+dev host. The skid
  isn't merely small — it's *phase-locked* to a handful of fixed offsets (27/30/31).
  That's a stronger property than "skid happens to be tiny."

- **Exports are deterministic by construction.** BTreeMap ordering means the
  text artifact and Prometheus exposition are reproducible; the Prometheus
  buckets are properly cumulative with a `+Inf` bucket, `_sum`, and `_count`,
  and that's locked by `exports_are_deterministic_and_cumulative`. The `sum`
  is a `u128` so it can't overflow across long runs.

- **The harness honors the iteration-16 throttle lesson.** The PERIODS floor of
  10k plus the arm-then-park-to-NEVER_FIRES discipline keeps the PMI rate
  (~14k/s measured) far under `perf_event_max_sample_rate` (77k). The module
  doc-comment explicitly cites the iteration-16 hazard, so the constant choice
  is traceable, not magic.

- **The live test is honest about its environment.** `skid_gate.rs` probes
  `/dev/kvm` and *skips* (returns, doesn't fail) when it isn't usable, while the
  real gate (`r.gate.expect(...)`) and an order-of-magnitude sanity bound
  (`< 200`) run when KVM is present. It passed 3/3 amid this session's load.

- **The measurement math is self-correcting.** Reading a free-running cumulative
  counter and computing `after − armed_point` (rather than resetting per sample)
  means inter-sample drift cannot bias the skid, and the `after < armed_point`
  guard turns a stale/early signal into a loud error rather than a silent
  underflow. Clean.

- **Clean separation of concerns per ARCH §1.** Collection logic
  (`SkidHistogram`) lives in dh-verify (the named home, unsafe-free); the VM
  machinery driver lives in dh-cli. The module headers cross-reference ARCH §9 /
  §3.2 and the bead. Good provenance.
