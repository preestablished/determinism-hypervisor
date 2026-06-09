# Phase 0 — Bootstrap

**Starting state:** nothing. No repos, no code, no provisioned hosts.

## Outcome

Every repo exists, builds green in CI on both architectures, and depends on a published
shared proto crate. Both hosts are provisioned and pass a preflight checklist. No
behavior exists yet — this phase buys the right to parallelize everything after it.

## Entry requirements

- Hardware access: the 64-bit Intel Linux box (VT-x enabled in firmware) and the DGX
  Spark (aarch64, Blackwell GPU, CUDA toolchain installable).
- Rust toolchain (stable, 2021+) on both hosts; cross-compile targets registered.
- A git host for the ten repos and a CI runner on each architecture.
- The design docs in `../docs/` (MAP.md + ten service doc sets) — every repo skeleton
  is scaffolded to match its doc set.

## Work, by repo (ordered)

**Step 1 — before everything else:**

| Repo | Milestone | Why first |
|---|---|---|
| `control-plane` | M0 — proto repo + `determinism-proto` crate | Explicitly "day 1; blocks everyone." All ten service APIs live in this repo's `proto/`; every other repo's M0 depends on importing the generated crate. Includes buf-style lint + breaking-change CI and a tagged v0.1.0. |

**Step 2 — all in parallel (each depends only on `determinism-proto` v0.1):**

| Repo | Milestone | Contents |
|---|---|---|
| `determinism-hypervisor` | M0 — workspace + KVM smoke | 8-crate workspace; proves KVM caps on the Intel box (no in-kernel irqchip, MSR filter, dirty ring, immediate-exit). |
| `snapshot-store` | M0 — skeleton, types, baselines | Workspace, core types, NVMe I/O baseline benchmarks. |
| `guest-sdk` | Milestone 0 — `detguest-wire` formats + golden tests | The `no_std` wire crate with byte-level golden tests. Zero external deps; unblocks hypervisor-side integration early. |
| `reference-workload` | M0 — workspace, schemas, protocol crate | Feature-map YAML schema (platform-canonical), harness protocol crate. |
| `state-scorer` | workspace portion of M1 | Workspace + proto wiring only; feature decoding proper lands in Phase 4. |
| `input-synthesizer` | M0 — scaffolding & contracts | Workspace, `InputModel` trait, burst wire types. |
| `exploration-orchestrator` | M0 — workspace, protos, skeleton | Workspace; `orch-core` declared pure (no tokio/tonic). |
| `replay-renderer` | M0 — skeleton (no behavior) | Two-binary workspace (`replayd` Spark / `reexec-agent` Intel). |
| `observatory` | M0 — skeleton & store | Workspace + SQLite/WAL store scaffold. |

**Step 3 — host provisioning (no repo; parallel with step 2):**

- Intel box: pin kernel version + microcode (the hypervisor's CI determinism strategy
  requires it); enable unprivileged `perf_event` access for the pinned
  instructions-retired counter; hugepages reserved; kernel config recorded as an
  artifact.
- DGX Spark: CUDA/NVRTC toolchain for candle/cudarc; NVENC-capable FFmpeg; verify an
  aarch64 Rust + CUDA hello-world kernel runs.
- Network: the two hosts reach each other on the 74xx port plan (MAP.md canonical
  table); TLS material generated for control-plane later.

## Cross-repo ordering

```
control-plane M0 (proto crate v0.1)
        │
        ├─► all nine other repo M0s          (parallel)
        └─► host preflight checklist          (parallel, no code dep)
```

## Exit gate

1. `cargo build` + `cargo test` green in all ten repos, in CI, on x86_64; aarch64 CI
   green for the repos that deploy to the Spark (scorer, replayd, control-plane,
   observatory, synthesizer).
2. `determinism-proto` v0.1.0 tagged; a sample client in each repo compiles against it.
3. `guest-sdk` wire-format golden tests pass (the platform's first real tests).
4. Both hosts pass the recorded preflight checklist (KVM caps, perf access, CUDA,
   NVENC, port reachability).

## Parallelism notes

After control-plane M0, all work in this phase is embarrassingly parallel — one agent
per repo plus one on host provisioning. Nothing in this phase touches another repo's
code.
