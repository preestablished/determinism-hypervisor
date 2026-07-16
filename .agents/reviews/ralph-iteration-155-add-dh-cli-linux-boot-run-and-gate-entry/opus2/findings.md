# Findings

## Important: Linux READY is accepted by stream id only; the Ready payload is neither validated nor compared by the gate

`tools/dh-cli/src/linux.rs:203`-`212` increments the `NextSdkEvent` feed when a drained detchannel event has stream `EventKind::Ready`, and stores only `(kind, payload.len())`. `tools/dh-cli/src/linux.rs:244`-`255` then treats any such event as a successful READY stop, regardless of whether the payload is the required 16-byte `Ready{unit, region_count, manifest_generation}` record or whether `region_count`/`manifest_generation` are sane. The Linux gate comparison goes through `ready_fingerprint` at `tools/dh-cli/src/linux.rs:268`-`279`, and that fingerprint omits even `ready_payload_len`.

That leaves `dh-cli gate --linux` able to report `Ready EventKind 14` and pass zero-divergence for a guest that emits a malformed/empty Ready stream-14 event. This is weaker than the M9 decision in `docs/decisions/m9-linux-ready-and-block-device.md:33`-`37`, which defines the accepted READY point as EventKind 14 with the Ready payload after channel initialization/Hello/Start/region registration. The existing worker acceptance test validates this shape in `crates/dh-worker/tests/linux_worker_api.rs:500`-`514` and asserts non-empty/stable payload fields at `crates/dh-worker/tests/linux_worker_api.rs:1104`-`1117`; the CLI path should apply equivalent validation or share a small parser.

Suggested fix: parse the Ready payload in `run_to_ready`, require length 16, require at least a nonzero `region_count`, and preferably require an even/stable `manifest_generation`. Include the parsed fields or a payload digest in `LinuxReadyReport` and `ready_fingerprint` so the gate artifact proves the compared READY record, not just the stream id.

## Coverage Gap: dh-cli tests cover Linux arguments but not Linux READY semantics or the Linux gate artifact

`tools/dh-cli/tests/cli_args.rs:65`-`174` pins argument parsing for Linux artifact flags, defaults, and nanokernel preservation, but no test exercises `tools/dh-cli/src/linux.rs`'s Ready event handling or `tools/dh-cli/src/gate.rs:98`-`104`'s Linux gate fingerprint construction. With `DH_M9_*` unavailable in this shell, the artifact-backed acceptance command could not be run, so the branch currently has no executable evidence that the new CLI path reports and compares the intended READY payload.

Suggested fix: add unit-level coverage around a small Ready-payload parser/fingerprint builder that does not need KVM or `DH_M9_*`, and keep the artifact-backed `DH_M9_ALLOW_SKIP=0 cargo run -p dh-cli -- gate --linux --runs 2 ...` as the live acceptance gate.
