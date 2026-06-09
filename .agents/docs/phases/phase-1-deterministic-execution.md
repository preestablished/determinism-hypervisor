# Phase 1 — Deterministic Execution (single timeline)

## Outcome

The platform can boot a guest on the Intel box and run it **bit-deterministically**:
the same boot + the same configuration, executed twice, produces an identical chained
state hash at the same retired-instruction count. Events land at exact instruction
boundaries. Separately, the snapshot page store works standalone against synthetic
data. No forking, no real workload yet — one timeline, perfectly repeatable.

This is the riskiest phase in the program: if instruction-precise determinism on KVM
can't be made to work, nothing downstream matters. Front-load it.

## Entry requirements

- Phase 0 exit gate (all skeletons build; Intel box preflight passed — pinned kernel,
  perf_event access, KVM caps).

## Work, by repo (ordered)

**`determinism-hypervisor` — the critical path:**

1. M1 — nanokernel guest + pv devices + boot from image.
2. M2 — `detclock`: retired-instruction counting + exact landing (PMI kick at
   target−8192 skid margin, single-step refinement). *Depends on M1.*
3. M3 — virtual time, deterministic injection, run control (run-until icount /
   virtual-ns / event; pause at deterministic boundary). *Depends on M2.*

**`snapshot-store` — parallel track (no hypervisor dependency):**

1. M1 — page store core (1 GiB append-only packs, sharded index, ingest). Uses the
   **synthetic-guest generator** from its test plan so it needs no real guest.
2. M2 — manifest codec + snapshot commit/resolve. *Depends on M1.*
3. M3 — metadata DB (`snapstore-meta`, SQLite schema, lineage queries). *Parallel
   with M2 after M1.*

**`guest-sdk` — parallel track:**

1. Milestone 1 — `detguest-host` over a mock `GuestMem` (host-side protocol crate;
   zero hypervisor dependency).
2. Milestone 2 — `detguest-agent` boots as PID 1, channel up, logs flow. *No
   hypervisor dependency: the agent is a Linux userspace binary and boots under
   guest-sdk's own minimal KVM test harness + kernel (per its plan); the hypervisor's
   Linux-guest path only arrives at M9 (Phase 3).*

**`reference-workload` — opportunistic early start (pure host-side Rust, zero deps):**

1. M1 — emulator core boots a test ROM (~3 weeks of work; starting it now keeps it
   off the Phase 3 critical path).

## Cross-repo ordering

```
hypervisor M1 ──► hypervisor M2 ──► hypervisor M3        (critical path)
      │
      └─► guest-sdk Ms2 (agent boots under hypervisor M1 guest)

snapshot-store M1 ──► M2, M3                              (independent)
guest-sdk Ms1                                             (independent)
reference-workload M1                                     (independent, early start)
```

## Exit gate

1. **Determinism gate:** boot the nanokernel guest, run to icount N twice → identical
   chained state hash; repeat with an injected timer event at an exact icount →
   identical hashes. 100 consecutive runs, zero divergence.
2. **Landing-precision gate:** events land at the requested retired-instruction count
   exactly (hypervisor M2 acceptance), including across REP-string boundaries.
3. snapshot-store M1/M2 benchmark gates met on synthetic data (≥1.5 GB/s fast-path
   ingest target, manifest round-trip property tests green).
4. guest-sdk agent boots in-guest and streams log events host-ward.

## Parallelism notes

Four independent tracks (hypervisor; snapshot-store; guest-sdk host crate; emulator
core). The hypervisor track is the long pole and should get the strongest
agent/engineer. Do not start hypervisor M4 (snapshots) until the determinism gate
above is green — snapshotting a nondeterministic VM produces unfalsifiable bugs.
