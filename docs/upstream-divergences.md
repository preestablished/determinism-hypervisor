# Upstream planning-tree divergence ledger (bead veu)

`.agents/docs/determinism-hypervisor/{API,ARCHITECTURE,IMPLEMENTATION-PLAN}.md` and
`.agents/docs/guest-sdk/ARCHITECTURE.md` are **synced from an upstream planning
tree** (last sync: commit `d55ecc3`). During implementation this repo found
twenty-nine accepted divergences where those documents, the cited upstream phase
docs, or the sibling reference-workload/guest-sdk planning docs are stale or
wrong; in every case **the code in this repo is authoritative** (it ships,
round-trips, and is pinned by tests). Eight were amended locally after the sync
and will be silently reverted by the next sync unless pushed; five are
upstream-only wording fixes (no local doc edit was needed because the code or a
decision doc is the authority); eight more were amended locally BEFORE the
`d55ecc3` sync and were **already silently reverted by it** — the revert hazard
this file exists to prevent is not theoretical, it has already happened once
(entries #11–#18; for those, the current local copies are stale again too, so
applying upstream + re-syncing fixes both sides). Entries #22–#29 are the M9
Linux accepted-drift ledger against the cited phase, hypervisor, guest-sdk, and
reference-workload planning docs under `~/.agents/projects/determinism`.

This file is the ready-to-apply artifact for whoever can write to the upstream tree:
each entry gives the exact old text, the exact new text (or proposed wording), and
the local commit / evidence trail. It lives in `docs/` (NOT `.agents/docs/`)
precisely so a sync cannot overwrite it.

Section/line references are to the upstream documents as of the `d55ecc3` sync; line
numbers may have drifted upstream — match on the quoted text.

Operator instructions:

- **If a quoted "Old" string is not found verbatim upstream, STOP on that entry and
  flag it for human reconciliation — do not guess an insertion point.** Upstream may
  have moved past the `d55ecc3` baseline. Entries #4, #5, #6, and #14–#17 quote
  multi-line blocks that must be matched whole.
- Bead IDs (`veu`, `4ld`, …) and iteration numbers are provenance only — they point
  at this repo's history and are not prerequisites for applying an entry.
- The long Markdown `| … |` table rows below are intentionally single-line; paste
  them unwrapped or the table cell breaks.
- Quick index — amended locally after the sync (exact diffs): #1, #2, #7, #9, #10,
  #19, #20, #21;
  upstream-only proposals: #3, #4, #5, #6, #8; amended locally before the sync and
  reverted by it (exact diffs recovered from `d55ecc3^`): #11–#18; M9
  accepted-drift entries: #22–#29.

---

## Divergences with a local amendment (sync WILL revert these — apply upstream first)

The "New" texts in this section are verbatim copies of review-passed local edits
(commits cited per entry). Once upstream applies an entry and `.agents/docs` is
re-synced, the local amendment is subsumed by the sync; when all entries in this
file are applied/resolved, bead `veu` can close.

### #1 — API.md §3.1: `[240..256)` reserved row → `encoder_fingerprint` + `reserved` split

- **Found:** iteration 61 review. **Local amendment:** commit `c7e2b1a`.
- **Why:** bead 4ld (closed 2026-06-10) repurposed `[240..248)` as the `u64`
  detguest-wire encoder fingerprint. Writer (`dhilog.rs` seal), reader
  (`reader.rs` `parse_header`) and golden tests all implement the split.

Old (upstream §3.1 segment-header table):

```
| 240 | 16 | `reserved` | zeros; readers MUST reject nonzero (reserved-means-zero rule) |
```

New:

```
| 240 | 8 | `encoder_fingerprint` | `u64` detguest-wire encoder fingerprint (bead 4ld); zero ⇒ no SDK digests in this segment. Verifiers compare fingerprints before SDK_EVENT digests to detect encoder skew |
| 248 | 8 | `reserved` | zeros; readers MUST reject nonzero (reserved-means-zero rule) |
```

### #2 — API.md §2.8: `SlotState` `PAUSED` → `PAUSED_S`

- **Found:** iteration 63, by real protoc codegen (bead bcb). **Local amendment:**
  commit `8a22a56`.
- **Why:** proto enum values use C++ scoping — they are siblings of the *package*,
  not the enum — so `SlotState.PAUSED` collides with `StopReason.PAUSED`; protoc
  rejects it. The spec's own `FAULTED_S` already works around the same rule.
  Wire-compatible (numbers unchanged).

Old (upstream §2.8):

```
enum SlotState { SLOT_UNSPECIFIED = 0; EMPTY = 1; PAUSED = 2; RUNNING = 3;
                 FROZEN = 4; FAULTED_S = 5; }
```

New:

```
enum SlotState { SLOT_UNSPECIFIED = 0; EMPTY = 1; PAUSED_S = 2; RUNNING = 3;
                 FROZEN = 4; FAULTED_S = 5; }
// PAUSED_S (like FAULTED_S): proto enum values use C++ scoping — siblings of
// the package, not the enum — so SlotState values must not collide with
// StopReason's PAUSED/FAULTED. protoc rejects a bare PAUSED here.
```

### #7 — API.md §4: `ENTR` row is VERSIONED (v1 = 56 B, v2 = 72 B)

- **Found:** iteration 71 review. **Local amendment:** commit `efa286f`.
- **Why:** the snapshot engine writes ENTR v2 = the spec's 56-byte PRNG state ‖ the
  pv-entropy device's guest-visible regs (`buf_gpa u64, len u32, status u32`:
  56 + 8 + 4 + 4 = 72 bytes), which
  have no §4 tag of their own. v1 remains the spec-exact 56-byte form. ChaCha20
  known-answer pins in dh-devices guard the PRNG semantics across dep upgrades.

Old (upstream §4 DHSNAP section table):

```
| `ENTR` | ChaCha20 PRNG state, exactly `seed: [u8;32], stream: u64, word_pos: u128` (56 bytes) — `rand_chacha`'s exportable state (`get_seed`/`get_stream`/`get_word_pos`); restore re-seeds via `set_stream`/`set_word_pos` and MUST reproduce the next draws bit-identically (golden test, IMPLEMENTATION-PLAN M4) |
```

New:

```
| `ENTR` | VERSIONED by `sec_version`. v1: ChaCha20 PRNG state, exactly `seed: [u8;32], stream: u64, word_pos: u128` (56 bytes) — `rand_chacha`'s exportable state (`get_seed`/`get_stream`/`get_word_pos`); restore re-seeds via `set_stream`/`set_word_pos` and MUST reproduce the next draws bit-identically (golden test, IMPLEMENTATION-PLAN M4). v2 (what the snapshot engine writes): the v1 56 bytes ‖ the pv-entropy device's guest-visible regs `buf_gpa u64, len u32, status u32` (72 bytes total) — the regs have no §4 tag of their own |
```

### #9 — ARCHITECTURE.md §8.2 + IMPLEMENTATION-PLAN.md M4: dirty-ring chaos size 512 → 1024

- **Found:** iteration 84, bead 28i (empirically, EINVAL on the lab box).
  **Local amendment:** commit `d94c605`.
- **Why:** the kernel reserves 64 + 512 (PML) ring entries on x86 and rejects rings
  below that floor — 1024 is the smallest legal ring. Also: the landed acceptance
  compares snapshot REFS (BLAKE3 over the manifest incl. the DHSNAP blob), which is
  equal-or-stronger than the originally-sketched hash comparison.

Old (upstream IMPLEMENTATION-PLAN M4 accept line):

```
- Dirty-ring-full forced (ring size 512) — hashes unchanged vs large ring.
```

New:

```
- Dirty-ring-full forced (smallest legal ring — 1024 on the lab box; the kernel's 64+512 PML reserved floor EINVALs 512) — snapshot refs unchanged vs large ring.
```

Old (upstream ARCHITECTURE §8.2 first bullet):

```
- Primary: **dirty ring** (`KVM_CAP_DIRTY_LOG_RING_ACQ_REL`, ring size 65536 entries)
```

New:

```
- Primary: **dirty ring** (`KVM_CAP_DIRTY_LOG_RING_ACQ_REL`, ring size 65536 entries;
  EMPIRICS, iteration 84: the kernel reserves 64 + 512 (PML) entries on x86 and
  rejects rings below that floor — 1024 is the smallest legal ring on the lab box,
  which is what the 28i chaos acceptance forces, not the originally-sketched 512)
```

### #10 — API.md §4: `NETL` row is 36 bytes of pure registers; the pending-RX rule holds by construction

- **Found:** iteration 85, bead mmv. **Local amendment:** commit `84e99cc`.
- **Why:** the landed pv-net device buffers NO frames — TX is drained per exit via
  `PvNet::tx_regs` by run control, RX delivery is immediate at record landing — so
  there is no pending-RX state to serialize and no enforcement code exists or is
  needed. The 36-byte regs-only layout is documented and pinned in
  `crates/dh-devices/src/net.rs` (layout doc + the
  `snapshot_restore_roundtrip_is_byte_identical` test).

Old (upstream §4 DHSNAP section table):

```
| `NETL` | pv-net regs + pending-RX state (must be empty at snapshot; enforced) |
```

New:

```
| `NETL` | pv-net registers only (36 bytes: tx_buf_gpa u64, tx_len u32, tx_status u32, rx_buf_gpa u64, rx_cap u32, rx_len u32, rx_vector u32). The original "pending-RX state (must be empty at snapshot; enforced)" is satisfied BY CONSTRUCTION: the device buffers no frames (TX is drained per exit by run control; RX delivery is immediate at record landing), so no pending state exists to serialize — iteration-85 amendment |
```

### #19 — API.md §3.3: `NET_RX` payload is 1–2048; zero-length is invalid

- **Found:** iteration 85 review (opus1 I2), fixed iteration 100, bead 206.
  **Local amendment:** this iteration's commit (record-kind table, `0x03` row).
- **Why:** upstream gave NET_RX no lower bound ("≤ 2048"), and the reader
  deliberately accepted zero-length frames — but `PvNet::apply_net_rx` rejects
  `len == 0` (`FrameTooBig`) and a TX doorbell faults on `tx_len == 0`, so a
  recorded empty NET_RX would be unreplayable. Bead 206 chose forbid-at-codec
  over invent-empty-delivery-semantics: the writer refuses
  (`WriteError::EmptyNetRx`), reader validation requires `1..=2048`, the device
  is unchanged. Authority: `crates/dh-inputlog/src/{dhilog,reader}.rs`,
  `crates/dh-devices/src/net.rs`.

Old (record-kind table, `0x03` row):

```
| `0x03` | `NET_RX` | raw frame bytes (`payload_len` is the frame length, ≤ 2048) |
```

New:

```
| `0x03` | `NET_RX` | raw frame bytes (`payload_len` is the frame length, 1–2048; zero-length is INVALID — the device rejects empty delivery, so writer and reader forbid it) |
```

### #20 — IMPLEMENTATION-PLAN.md M4: perf gates accepted-as-measured (snapshot < 150 ms, restore < 450 ms)

- **Found:** iteration 99 (bead 9sb instruments + measurement). **Decision:**
  bead 8ot, operator call 2026-06-12 — option (d), accept-as-measured.
  **Local amendment:** this commit (M4 "Perf gates" bullet). **Superseded
  locally by #21:** the 2026-06-17 reference-machine decision downgrades
  latency caps to telemetry.
- **Why:** the original snapshot/restore numbers (15 ms / 150 ms at 8k pages /
  128 MiB) imply > 2 GB/s durable bandwidth; the box's ext4 LV sustains
  ~350 MB/s durable (`dd conv=fsync` floor 96–200 ms / 32 MiB), and the store's
  put is a durability receipt (R12) that cannot beat the disk. Engines are not
  the bottleneck (fork p50 326 µs passes with 30× headroom; defeating store
  dedup moved snapshot p50 < 10%). Correctness outranks speed: gates were reset
  to the measured baseline + ~45% variance headroom and now act as REGRESSION
  gates; the original numbers are retained as improvement targets (backlog
  bead). M7's ≤ 100 ms exploration-step budget is flagged in the amendment as
  in tension with these numbers. Authority:
  `crates/dh-worker/tests/perf_gates.rs` (decision record at the constants).

Old (M4 "Accept" list, perf-gates bullet):

```
- Perf gates (p50 on the box, 128 MiB demo guest — MAP.md canonical figure):
  fork < 10 ms, incremental snapshot ≤ 8k dirty pages < 15 ms, tier-B warm restore
  < 150 ms.
```

New: same bullet with snapshot < 150 ms / restore < 450 ms, the
ACCEPTED-AS-MEASURED rationale, the measured baselines, and the M7
exploration-step tension note (quoted in full in the amendment).

### #21 — IMPLEMENTATION-PLAN.md M4/M7: storage latency caps are telemetry on the reference machine

- **Found:** iteration 142 / bead 3sp, after page-channel GET_BATCH adoption
  still left restore above the accepted-as-measured cap on the current Linux
  KVM reference host. **Decision:** operator call 2026-06-17 — the current
  machine is the reference environment and may be slow. Correctness,
  determinism, and durable store receipts outrank latency.
- **Why:** the real snapshot-store path gives durability receipts and shares
  the reference host's storage and scheduler behavior. A hard snapshot,
  restore, or joint exploration-step p50 cap rejects a deterministic and
  correct system for environmental speed. Keep measuring the same surfaces,
  but do not make latency an acceptance failure.
- **Authority:** `crates/dh-worker/tests/perf_gates.rs` exercises fork,
  8k-page incremental snapshot, and full restore through the real store,
  asserts the correctness/page-count invariants, and prints p50/min/max as
  telemetry. `docs/phase-2-exit-gate.md` records the policy.

Old (M4 perf acceptance after #20 plus M7 storage budget language):

```
Perf gates (p50 on the box, 128 MiB demo guest): fork < 10 ms;
incremental snapshot < 150 ms; tier-B warm restore < 450 ms.

The JOINT exploration-step storage budget <= 100ms p50, verified
end-to-end on the quiesced box - fork -> run -> TakeSnapshot -> store
durability ack measured as one step.
```

New:

```
Snapshot, restore, fork, and joint exploration-step latency are
reference-machine telemetry. Acceptance requires deterministic,
correct execution and durable store receipts; the perf harness reports
p50/min/max but does not fail solely because the reference machine is
slow.
```

---

## Upstream-only wording fixes (no local doc edit; code / decision doc is the authority)

Unlike the section above, the "Proposed new" texts here are newly authored for this
ledger (accurate against the cited code, but not themselves review-passed doc edits)
— upstream is free to rewrap or rephrase as long as the technical content survives.

### #3 — API.md §4: `EVTC` row understates the implemented contents

- **Found:** iteration 65 review. **Authority:** `crates/dh-devices/src/detchannel.rs`
  (`EVTC_V1_LEN = 39`, `EVTC_LEN = 43`, `EVTC_VERSION = 2`; ships and
  round-trips, pinned by the `evtc_roundtrips_attached_state_and_seqs` and
  `evtc_restore_between_inject_out_and_in_preserves_pending_query` tests in the
  same file).
- The row says the section is just the channel base GPA, but EVTC carries:
  `init_lo u32, init_hi u32, init_status u32` (offsets 0/4/8), `inject_iseq`
  flag u8 + u32 (12..17), `last_quiesce_ack` flag u8 + u32 (17..22), then channel
  flag u8 + `gpa u64` + producer seqs `ring_c u32, ring_i u32` (22..39). The ring,
  manifest, and index state genuinely live in guest RAM (that part of the row is
  right); the host-side latch/seq state above does not and must be serialized.
  EVTC v2 appends `pending_inject_count u32` at 39..43 followed by sorted
  variable-length entries (`iseq u32 | name_id u32 | name_len u32 | name
  bytes`) for drained but unanswered InjectQuery records; `name_len =
  u32::MAX` means no resolved interned name was available. This preserves the
  restore window between `OUT PORT_INJECT` and the matching `IN PORT_INJECT`,
  including name-specific FaultPlan decisions. EVTC v1 remains
  restore-compatible for legacy 39-byte sections but new snapshots use v2.

Old (upstream §4):

```
| `EVTC` | detchannel attach state: channel base GPA `u64` (0 = not attached). All ring, manifest, and index state lives in guest RAM and travels with the pages (guest-sdk ARCHITECTURE §2); the host re-attaches at this GPA after restore |
```

Proposed new:

```
| `EVTC` | detchannel host state (v2): 43-byte base `init_lo u32, init_hi u32, init_status u32`, `inject_iseq` flag u8 + u32, `last_quiesce_ack` flag u8 + u32, attach flag u8 + channel base GPA `u64` + producer seqs `ring_c u32, ring_i u32`, then `pending_inject_count u32` plus sorted variable-length entries `iseq u32 | name_id u32 | name_len u32 | name bytes` for drained but unanswered InjectQuery records (`name_len = u32::MAX` means no resolved interned name). Attach flag 0 means not attached and pending count must be 0. Ring, manifest, and index state lives in guest RAM and travels with the pages (guest-sdk ARCHITECTURE §2); the host re-attaches at the recorded GPA after restore, reinstates the non-reconstructible producer seqs, and preserves the OUT/restore/IN inject-answer window including name-specific FaultPlan decisions. Authoritative layout: `dh-devices/src/detchannel.rs` (`EVTC_V1_LEN`/`EVTC_LEN`/`EVTC_VERSION`) |
```

### #4 — ARCHITECTURE.md §2 lifecycle one-liner: `Running → Frozen` should be `Paused → Frozen`

- **Found:** iteration 66 review. **Authority:** the implemented (and §8.4-correct)
  state machine — fork requires a *paused* parent; a running slot is never frozen
  directly.

Old (upstream, slot-lifecycle bullet):

```
- A slot's lifecycle: `Empty → Created (CreateVm/RestoreSnapshot/Fork) → Paused ⇄
  Running → Frozen (parent of live CoW children) → Empty (DestroyVm)`. All RPCs carry
```

Proposed new (only the transition into Frozen changes — it leaves from Paused):

```
- A slot's lifecycle: `Empty → Created (CreateVm/RestoreSnapshot/Fork) → Paused ⇄
  Running`, `Paused → Frozen (parent of live CoW children) → Empty (DestroyVm)`. All RPCs carry
```

(The chain is deliberately split into two inline-code spans: there is no
`Running → Frozen` edge, so do not "tidy" it back into one arrow chain.)

### #5 — ARCHITECTURE.md §2.2 + §8.2: `KVM_MEM_LOG_DIRTY_PAGES` is required on BOTH paths

- **Found:** iteration 67, bead ygt — EMPIRICALLY A/B-proven on a 6.8 kernel
  (0 ring entries harvested without the flag vs ≥3 with it). **Authority:**
  `crates/dh-vmm/src/dirty.rs` (`enable_dirty_logging`).
- The kernel publishes dirty-RING entries only for dirty-tracked memslots; the ring
  and the bitmap differ in *retrieval* (`KVM_RESET_DIRTY_RINGS` vs
  `KVM_GET_DIRTY_LOG`, which the ring forbids), not in the memslot flag.

Old (upstream §2.2):

```
  `KVM_EXIT_MMIO`). `KVM_MEM_LOG_DIRTY_PAGES` is set on the RAM memslot **only on the
  bitmap fallback path** — the dirty ring and the dirty bitmap are mutually exclusive
  per VM (enabling the ring forbids `KVM_GET_DIRTY_LOG`; setting both is an EINVAL
  trap for the implementer — see §8.2). Layout:
```

Proposed new:

```
  `KVM_EXIT_MMIO`). `KVM_MEM_LOG_DIRTY_PAGES` is set on the RAM memslot **on both
  dirty-tracking paths** — the kernel publishes dirty-ring entries only for
  dirty-tracked memslots (empirically A/B-proven: without the flag the ring stays
  empty). Ring and bitmap are mutually exclusive per VM in RETRIEVAL only (enabling
  the ring forbids `KVM_GET_DIRTY_LOG` — see §8.2). Layout:
```

Old (upstream §8.2, fallback bullet):

```
- Fallback: bitmap `KVM_GET_DIRTY_LOG` + `KVM_CLEAR_DIRTY_LOG`
  (manual-protect mode) over the RAM region — only on this path is
  `KVM_MEM_LOG_DIRTY_PAGES` set on the memslot (the ring and the bitmap are mutually
  exclusive per VM, §2.2).
```

Proposed new:

```
- Fallback: bitmap `KVM_GET_DIRTY_LOG` + `KVM_CLEAR_DIRTY_LOG`
  (manual-protect mode) over the RAM region. `KVM_MEM_LOG_DIRTY_PAGES` is set on the
  memslot on BOTH paths (the ring needs it too — the kernel only publishes ring
  entries for dirty-tracked memslots); ring and bitmap are mutually exclusive per VM
  in retrieval only (§2.2).
```

### #6 — ARCHITECTURE.md §4 defense 4: TSC is aligned via a one-time offset at restore, not on every VM entry

- **Found:** iteration 70. **Authority:** `docs/decisions/tsc-alignment.md`
  (measured, decided 2026-06-10) — `KVM_VCPU_TSC_OFFSET` attribute set ONCE at
  restore; `guest_tsc = host_tsc + offset` holds across entries with no per-entry
  write. The upstream text already carries a caveat pointing toward the offset
  approach; the normative sentence should match the decision.

Old (upstream §4, defense list item 4 — replace the item in its ENTIRETY; the
"benchmark both in M3" closing clause is deliberately dropped in the new text
because the benchmark happened and the decision is frozen):

```
  4. On **every VM entry** after an exit, the VMM aligns the guest TSC to `vns` at the
     entry boundary, so even a stray kernel RDTSC reads a value that is *approximately*
     virtual and drifts only between exits. **Caveat:** per-entry value writes via
     `KVM_SET_MSRS{IA32_TSC}` can engage KVM's TSC-offset synchronization/matching
     heuristics (KVM treats small-delta guest TSC writes specially) and cost
     measurably at ~3k exits/guest-second — prefer adjusting the **TSC offset**
     (`KVM_VCPU_TSC_CTRL` offset attribute) over MSR value writes; benchmark both in
     M3 before freezing the mechanism.
```

Proposed new (rewrite item 4's opening to match the decision; keep the offset
preference as the normative mechanism rather than a caveat):

```
  4. At restore, the VMM sets the **TSC offset once** via the `KVM_VCPU_TSC_OFFSET`
     device attribute so `guest_tsc = host_tsc + offset` aligns the guest TSC to
     `vns`; the relation holds across VM entries with no per-entry write, so even a
     stray kernel RDTSC reads a value that is *approximately* virtual. Per-entry
     value writes via `KVM_SET_MSRS{IA32_TSC}` are explicitly avoided: they engage
     KVM's TSC-offset synchronization/matching heuristics (KVM treats small-delta
     guest TSC writes specially) and cost measurably at ~3k exits/guest-second
     (measured decision: docs/decisions/tsc-alignment.md, 2026-06-10).
```

### #8 — ARCHITECTURE.md §8.5: the state-hash vCPU preimage is NOT the DHSNAP VCPU section bytes

- **Found:** iteration-70 review (item I1); decided during iteration 73's snapshot
  engine work (bead qmp). **Authority:** `crates/dh-worker/src/snapshot_engine.rs`
  (decision documented at the top of the module: "HASH vs SECTION reconciliation
  (iteration-70 review I1, decided here)").
- Decision (option b): the two stay SEPARATE. The hash keeps the field-selective
  `canonical_vcpu_blob` (padding-excluded; every existing chain value depends on
  it); the DHSNAP VCPU section is the raw-struct restore codec. Folding raw structs
  into the hash would re-import the reserved-byte hazard class iteration 69
  eliminated.

Old (upstream §8.5 hash definition):

```
        || canonical vCPU blob (DHSNAP vCPU section bytes, canonicalized §8.1)
```

Proposed new:

```
        || canonical vCPU blob (field-selective, padding-excluded — NOT the DHSNAP
           VCPU section bytes; the section is the raw-struct restore codec and is
           hashed only indirectly via the manifest. See dh-worker snapshot_engine.rs)
```

---

## Divergences already reverted once by the `d55ecc3` sync (apply upstream, then re-sync)

These eight were amended locally in iterations 8–51, before the `d55ecc3` sync; the
sync overwrote them with the upstream (stale) text, so the CURRENT local copies are
stale again as well. The "New" texts below are the exact pre-sync amendment texts,
recovered mechanically from `git diff d55ecc3^ d55ecc3 -- .agents/docs/` (each "Old"
block is the `+` side of that revert, i.e. today's upstream/local text; each "New"
block is the `-` side, i.e. what review-passed iterations had written). Every entry's
claim was re-verified against the current code on 2026-06-12 — all still hold.

### #11 — API.md §2.1: `skid_margin` is EXCLUDED from the `machine_config_hash` preimage

- **Found:** iteration 9 review. **Pre-sync amendment:** commit `354aa7f`.
- **Why:** landing knobs must not fork snapshot identity. Pinned by the
  `landing_knobs_do_not_fork_identity` test in `crates/dh-vmm/src/config.rs` (which
  asserts `skid_margin` — and the later-added `resync_slack` — leave
  `machine_config_hash` unchanged).

Old (upstream §2.1 `MachineConfig`):

```
  uint32 skid_margin      = 10;  // default 8192 (landing only; does not affect results)
```

New:

```
  uint32 skid_margin      = 10;  // default 8192 (landing only; does not affect results;
                                 //   for that reason it is EXCLUDED from the canonical
                                 //   MachineConfig encoding / machine_config_hash preimage
                                 //   — landing knobs must not fork snapshot identity)
```

### #12 — API.md §3.1/§3.3: `HAS_AUX` excludes the terminal `END`; `END`'s AUX/boundary semantics

- **Found:** iteration 8 review. **Pre-sync amendment:** commit `19c3ace`.
- **Why:** `END` is itself an AUX-flagged record, so without the carve-out a log
  whose only AUX record is the terminal `END` would ambiguously set `HAS_AUX`.
  Authority: `crates/dh-inputlog/src/reader.rs` (validates "`flags.HAS_AUX`
  disagrees with the records (END does not count)").

Old (upstream §3.1 header table):

```
| 12 | 4 | `flags` | bit0 `SEALED` (complete, hashes valid); bit1 `HAS_AUX` (AUX records present); bit2 `EPOCH_HASHES` (AUX includes EPOCH_HASH records); others 0 |
```

New:

```
| 12 | 4 | `flags` | bit0 `SEALED` (complete, hashes valid); bit1 `HAS_AUX` (AUX records beyond the terminal `END` present); bit2 `EPOCH_HASHES` (AUX includes EPOCH_HASH records); others 0 |
```

Old (upstream §3.3 record table):

```
| `0x7F` | `END` | `stop_reason: u8` (mirrors proto StopReason), `_pad: [u8;7]`, `end_state_hash: [u8;32]` — always last record, always present in sealed logs |
```

New:

```
| `0x7F` | `END` | `stop_reason: u8` (mirrors proto StopReason), `_pad: [u8;7]`, `end_state_hash: [u8;32]` — always last record, always present in sealed logs. Carries `rflags.AUX = 1` and `boundary_rip = 0`: a minimal replayer that skips AUX records still terminates correctly via the header's `end_icount`/`end_state_hash`. `END` does not count toward `flags.HAS_AUX` |
```

### #13 — ARCHITECTURE.md §2.1 caps table: `KVM_CAP_TSC_CONTROL` → `KVM_CAP_VCPU_ATTRIBUTES`

- **Found:** iteration 14 (empirics 2026-06-09, lab Coffee Lake). **Pre-sync
  amendment:** commit `44a170f`. Companion to #6 (same TSC-offset decision).
- **Why:** `KVM_CAP_TSC_CONTROL` is TSC *frequency scaling* — absent on the lab box
  and never needed (single pinned host, no migration); restore normalizes TSC via
  the offset vCPU attribute. Authority: `crates/dh-vmm/src/kvm.rs` (the
  REQUIRED_RAW_CAPS list, with this exact empiric in the comment at line ~75).

Old (upstream §2.1 caps table row):

```
| `KVM_CAP_GET_MSR_FEATURES`, `KVM_CAP_TSC_CONTROL` | TSC normalization on restore |
```

New:

```
| `KVM_CAP_GET_MSR_FEATURES`, `KVM_CAP_VCPU_ATTRIBUTES` (TSC offset vCPU attr) | TSC normalization on restore via OFFSET writes (§4.4). Empirics 2026-06-09: `KVM_CAP_TSC_CONTROL` (TSC *frequency scaling*) was listed here but is absent on the lab Coffee Lake and never needed — single pinned host, no migration |
```

### #14 — ARCHITECTURE.md §3.1 + IMPLEMENTATION-PLAN M2: VM-exiting instructions retire ZERO instructions, not "exactly once"

- **Found:** iterations 47–50 (beads 0sc/20g/5l7 reconciliation + counting_semantics
  acceptance), MEASURED. **Pre-sync amendments:** commits `881d8e1`, `9557e53`,
  `3488efc` (the text below is the final form). This is the most load-bearing entry
  in this file: the upstream spec's normative retirement claim is empirically false.
- **Why:** exiting instructions exit before retirement and KVM completes them
  host-side by skipping `RIP`, which an `exclude_host=1` counter never sees.
  Authority: `tests/nanokernel/src/lib.rs` (`COUNTING_DELTA_AT_OUT_EXITS`) and
  `tests/determinism/tests/counting_semantics.rs` (bit-stable across cold
  boots/cores/processes/load).

Old (upstream §3.1, replace the whole bullet):

```
- `CPUID`, `HLT`, MMIO-exiting instructions each retire exactly once, on the resume
  that completes them. The boundary engine treats an instruction that has exited
  mid-emulation (`KVM_EXIT_MMIO` not yet completed) as **not yet retired**.
```

New:

```
- VM-exiting instructions retire **zero** guest instructions. MEASURED in isolation
  on the kvm-intel class for `CPUID`, PIO `OUT`, MMIO read, MMIO write, and `HLT`
  (counting guest + the counting_semantics single-step attribution: every park-loop
  hlt/jmp cycle advances the counter by exactly 1 — the jmp alone; see
  `nanokernel::COUNTING_DELTA_AT_OUT_EXITS`); PIO `IN` is EXPECTED to follow the
  same mechanism but is not yet isolated (constrained by the bit-identical icounts
  of IN-heavy boots). The mechanism: the instruction exits before
  retirement and KVM completes it host-side by skipping `RIP`, which an
  `exclude_host=1` counter never sees. (An earlier revision of this section claimed
  "retire exactly once, on the completing resume"; the empirics refuted that.)
  The boundary engine treats an instruction that has exited mid-emulation
  (`KVM_EXIT_MMIO` not yet completed) as **never retiring**: the count is the same
  before the exit and after the completing resume. Like the interrupt rule, this is a
  per-determinism-class measurement — re-validate per class, never assume across
  classes.
```

Old (upstream IMPLEMENTATION-PLAN, M2 accept):

```
- `counting_semantics` test: single-step a known 1,000-instruction nanokernel sequence
  (including REP MOVS, CPUID, MMIO exits); counter delta exactly 1,000; REP retires
  as 1.
```

New:

```
- `counting_semantics` test: single-step a known 1,000-instruction nanokernel sequence
  (including REP MOVS, CPUID, MMIO exits); counter delta exactly the region minus its
  VM-exiting instructions (ARCH §3.1 measured rule: exiting instructions retire zero —
  997 for the shipped guest, `nanokernel::COUNTING_DELTA_AT_OUT_EXITS`); REP retires
  as 1.
```

### #15 — ARCHITECTURE.md §3.2 landing loop: re-assert guest_debug per exit + the PLATEAU RULE

- **Found:** iteration 50 (measured, 240 cold boots). **Pre-sync amendments:**
  commits `3488efc` + `483f37f` (the text below is the final form).
- **Why:** an MMIO-WRITE exit eats the pending single-step trap (the emulator
  completes the instruction and clears TF without delivering the #DB) — an
  un-re-armed step free-runs. And targets on a zero-retirement plateau always land
  at the FIRST `(icount, RIP)` of the plateau. Authority:
  `crates/dh-vmm/src/boundary.rs` (the re-arm-on-every-exit logic and its comments,
  ~lines 154–186).

Old (upstream §3.2 landing-loop pseudocode annotation):

```
      (REP rule: if RIP unchanged, continue stepping without counting a boundary)
```

New:

```
      (REP rule: if RIP unchanged, continue stepping without counting a boundary;
       re-assert guest_debug after every handled exit — an MMIO-WRITE exit eats the
       pending single-step trap: the emulator completes the instruction and clears
       TF without delivering the #DB, and an un-re-armed step would free-run.
       PLATEAU RULE (measured, 240 cold boots): a target on a zero-retirement
       plateau — several consecutive exiting instructions sharing one icount —
       always lands at the FIRST (icount, RIP) of the plateau: the engine breaks at
       the first loop-top count==target observation, which is RIP-deterministic
       because the instruction stream is; skid variance never moves it)
```

### #16 — ARCHITECTURE.md §2.3: ELF boot CR4 carries OSFXSR/OSXMMEXCPT; OSXSAVE stays OFF as a determinism decision

- **Found:** iteration 51, bead ttk (live-proven: an SSE2 guest triple-faults
  without OSFXSR). **Pre-sync amendment:** commit `ad7185a`.
- **Why:** compiled (Rust/C x86_64 ABI) guests emit SSE2 by default; with OSXSAVE
  off there is no XSAVE/AVX surface, so guest FP state is exactly the x87+SSE set
  `KVM_GET_FPU` captures into the §8.1 hash blob. Authority:
  `crates/dh-vmm/src/boot.rs` (~line 241: `sregs.cr4 = PAE | OSFXSR | OSXMMEXCPT`)
  and `crates/dh-vmm/src/cpuid.rs` (the matching XSAVE/AVX feature-bit mask).

Old (upstream §2.3, guest type 1):

```
   (CR0/CR4/EFER/GDT set via `KVM_SET_SREGS`), `RIP = e_entry`, `RSI = &BootInfo`
```

New:

```
   (CR0/CR4/EFER/GDT set via `KVM_SET_SREGS`; CR4 carries PAE + OSFXSR/OSXMMEXCPT so
   compiled guests' baseline SSE2 works — OSXSAVE stays OFF as a determinism decision:
   no XSAVE/AVX surface exists, so guest FP state is exactly the x87+SSE set that
   `KVM_GET_FPU` captures into the §8.1 hash blob, and the §7.2 mask clears the
   XSAVE/AVX feature bits to match), `RIP = e_entry`, `RSI = &BootInfo`
```

### #17 — ARCHITECTURE.md §6.2: `TIMER_DEADLINE` is ABSOLUTE guest vns; `TimerArm` conversion is the caller's

- **Found:** iteration 47 (reviewed and refined in `9557e53`). **Pre-sync
  amendments:** commits `881d8e1` + `9557e53` (final form below).
- **Why:** the deadline shares the `VNS_LO/HI` clock, never segment-relative;
  run control's internal `TimerArm` carries counter-space vns and the caller
  subtracts the segment vns base. Authority: `crates/dh-devices/src/clock.rs`
  (absolute `timer_deadline_vns`) and `crates/dh-vmm/src/runctl.rs` (`TimerArm`).

Old (upstream §6.2):

```
- `0x18 TIMER_DEADLINE` (RW, 8B): vns deadline; write 0 disarms. One-shot.
```

New:

```
- `0x18 TIMER_DEADLINE` (RW, 8B): vns deadline; write 0 disarms. One-shot. The
  deadline is **ABSOLUTE guest vns** (the same clock `VNS_LO/HI` reads), never
  segment-relative (mirrors §6.4's `at_frame` convention). Run control's internal
  `TimerArm` carries counter-space (origin-0) vns; the conversion is the CALLER's
  subtraction of the segment vns base when reading the device's absolute deadline —
  a no-op until restore gives segments a nonzero base (see `runctl.rs` `TimerArm`).
```

### #18 — guest-sdk ARCHITECTURE.md, channel memory layout: ring W data is 0x100000 bytes (power of two), not 0x1E0000

- **Found:** iteration 47 (beads 0sc/20g/5l7 vendored-doc reconciliation).
  **Pre-sync amendment:** commit `881d8e1`. NOTE: this entry is in the
  **guest-sdk** doc set, not determinism-hypervisor — apply it to the guest-sdk
  ARCHITECTURE in the planning tree.
- **Why:** ring indices are free-running and mask-wrapped, so ring sizes MUST be
  powers of two; 1,966,080 (0x1E0000) is not one. Authority:
  `../guest-sdk/crates/detguest-wire/src/header.rs` (`RING_W_SIZE: u32 = 0x10_0000`
  with `is_power_of_two()` static asserts and the layout-offset asserts).

Old (upstream guest-sdk ARCHITECTURE, channel layout block):

```
0x020000  ring W data (1,966,080 bytes = 0x1E0000)
0x200000  end
```

New:

```
0x020000  ring W data (1,048,576 bytes = 0x100000)
0x120000  reserved (unused page tail; ring sizes are powers of two)
0x200000  end
```

---

## M9 accepted divergences from phase and sibling docs

Entries #22–#29 are not `.agents/docs` sync diffs from `d55ecc3`; they are the
M9 Linux local decisions and gate evidence that differ from the cited upstream
planning tree at `~/.agents/projects/determinism`. They are still ready-to-apply
upstream amendments: each names the stale upstream text or contract, the local
amendment, the authority files/tests, and the rollback or follow-up path.

### #22 — reference-workload `virtio-blk` game image → deterministic pv-blk exposed as `/dev/vdb`

- **Old upstream contract:** `reference-workload/API.md` `WorkloadImage` lists
  `machine.devices` as:

```
- { kind: virtio-blk, role: game-image, readonly: true, required: true }
```

  `reference-workload/ARCHITECTURE.md` also says the harness receives the game
  image at `/dev/vdb` as "read-only virtio-blk".
- **Local amendment:** M9 uses this repo's deterministic pv-blk device at MMIO
  `0xD000_4000`. The Linux guest driver/shim exposes that deterministic device
  as `/dev/vdb`, so the reference-workload `LoadGame{/dev/vdb}` and boot.toml
  `game_dev = "/dev/vdb"` contract is preserved without implementing virtio-blk.
  A deterministic virtio-blk subset is out of scope for M9.
- **Authority:** `docs/decisions/m9-linux-ready-and-block-device.md`;
  `docs/ops/test-partitioning.md` M9 artifact contract; `crates/dh-devices/src/blk.rs`;
  `tests/determinism/tests/linux_fixture_contract.rs`;
  `crates/dh-worker/tests/linux_worker_api.rs`; beads 4s9.5 and 4s9.30.
- **Rollback / follow-up:** a superseding virtio-blk or multi-disk bead must own
  the full deterministic device contract, including snapshot sections, state-hash
  inputs, replay/VerifyReplay semantics, fixture probes, and this ledger entry.

### #23 — M9 worker `base_image_hash` is the game image, not the fixture base image

- **Old upstream contract:** the reference-workload manifest has separate
  `artifacts.kernel` and `artifacts.initramfs`, then a `machine.devices` game
  image device. The M9 operational staging added both `DH_M9_BASE_IMAGE` and
  `DH_M9_GAME_IMAGE`, which can look like two guest block backings.
- **Local amendment:** current M9 worker configs have one pv-blk backing for the
  Linux game image. `MachineConfig.base_image_hash` is the BLAKE3 hash of
  `DH_M9_GAME_IMAGE`; `DH_M9_BASE_IMAGE` is fixture context used by the Linux
  fixture and staging helpers, not the pv-blk backing selected in the worker
  `MachineConfig`.
- **Authority:** `crates/dh-worker/tests/linux_worker_api.rs` asserts
  `MachineConfig.base_image_hash must be DH_M9_GAME_IMAGE` and separately checks
  the `DH_M9_BASE_IMAGE` fixture hash; `docs/ops/test-partitioning.md` documents
  both env vars and the `DH_M9_IMAGE_CACHE` staging contract; bead 4s9.30.
- **Rollback / follow-up:** a future two-disk, writable-root, or virtio-blk schema
  must add an explicit MachineConfig/proto distinction between the root/base image
  and read-only game image, then update worker tests, gate docs, and this entry.

### #24 — Linux READY is EventKind 14 on detchannel, not serial or ad hoc readiness

- **Old upstream contract:** guest-sdk and reference-workload docs describe the
  deterministic READY point after channel init, `Hello`, the autostart control
  leg, `LoadGame{/dev/vdb}`, `Start{}`, and expected-region registration. Those
  docs are correct about the ordering, but they do not explicitly reject serial
  console markers or other local M9 shortcuts as readiness evidence.
- **Local amendment:** the only accepted M9 Linux READY evidence is guest-sdk
  EventKind 14 `Ready{unit, region_count, manifest_generation}` on detchannel
  after the channel, control, and expected-region work is complete. Serial-only
  markers, console text, and ad hoc MMIO flags do not satisfy Linux READY for
  M9 gates. This does not remove the reference-workload control leg; the local
  fixture still requires `game_dev = "/dev/vdb"` and the Hello/LoadGame/Start
  path before Ready where applicable.
- **Authority:** `docs/decisions/m9-linux-ready-and-block-device.md`;
  `docs/ops/test-partitioning.md`; `tests/determinism/tests/linux_ready.rs`;
  `tools/dh-cli/src/gate.rs`; `docs/phase-1-exit-gate.md` M9 Linux rollup;
  beads 4s9.23 and 4s9.24.
- **Rollback / follow-up:** any alternate readiness signal must be specified in
  guest-sdk wire docs, logged in replay/VerifyReplay evidence, and accepted by a
  superseding M9 decision before gate tests can consume it.

### #25 — M9 Linux command line baseline changed from `dh-pvclock` plan to jiffies/no-TSC policy

- **Old upstream contract:** `determinism-hypervisor/ARCHITECTURE.md` §2.3
  gives this canonical baseline:

```
console=ttyS0 nokaslr norandmaps random.trust_cpu=off tsc=unstable clocksource=dh-pvclock nohz=off highres=off init=/init
```

  `reference-workload/API.md` says WorkloadImage `boot.cmdline` only appends
  extras such as `quiet`.
- **Local amendment:** M9 forces these exact bytes before any allowed extras:

```
console=ttyS0 nokaslr norandmaps random.trust_cpu=off random.trust_bootloader=on page_alloc.shuffle=0 notsc tsc=unstable clocksource=jiffies vdso=0 lpj=4096 noapictimer default_hugepagesz=2M hugepagesz=2M hugepages=1 init=/init
```

  `BzImageBoot.cmdline` remains append-only, but the only accepted extras are
  `quiet` and `loglevel=<n>` for `n` in `0..=7`; duplicate, conflicting, empty,
  non-ASCII, NUL-containing, or unsupported tokens are config errors before
  MachineConfig hashing.
- **Authority:** `docs/decisions/m9-linux-cmdline-policy.md`;
  `crates/dh-vmm/src/config.rs`; `crates/dh-worker/src/proto_map.rs`;
  `proto/hypervisor.proto`; Linux Phase 1 evidence in
  `docs/phase-1-exit-gate.md`.
- **Rollback / follow-up:** upstream can move back toward `dh-pvclock`,
  `nohz=off`, or `highres=off` only after those clock/timer surfaces exist in the
  deterministic device model and pass the Linux Phase 1 timer/landing gates.

### #26 — `LAPC` is a v2 deterministic userspace lAPIC section with mandatory replay coverage

- **Old upstream contract:** `determinism-hypervisor/API.md` §4 has a generic row:

```
| `LAPC` | lapic-stub state (Rust struct, fixed-layout encode) |
```

  Architecture text says snapshots contain the lapic-stub plus every DetDevice
  section, but does not pin the concrete section version or restore/replay
  semantics for M9 Linux.
- **Local amendment:** M9 ships `LAPC` `sec_version = 2`, a deterministic
  userspace xAPIC/lAPIC typed section. Legacy empty LAPC v1 is compatibility
  input only; current snapshots carry LAPC v2, state hashing frames LAPC
  tag/version/length/content, restore rejects malformed LAPC, and replay plus
  VerifyReplay fail on deliberate lAPIC mutations.
- **Authority:** `crates/dh-snapshot/src/dhsnap.rs`;
  `crates/dh-snapshot/tests/golden.rs` and
  `crates/dh-snapshot/tests/fixtures/v1_kitchen_sink_lapc_v2.dhsnap`;
  `crates/dh-vmm/src/lapic.rs`; `crates/dh-vmm/src/hash.rs`;
  `crates/dh-worker/tests/lapc.rs`; `crates/dh-worker/tests/restore_engine.rs`;
  `crates/dh-worker/tests/replay_engine.rs`; bead 4s9.17.
- **Rollback / follow-up:** any LAPC v3 or removal of the lAPIC model requires a
  DHSNAP format amendment, new golden fixture names or hashes, restore/replay
  compatibility tests, and updates to the state-hash preimage docs.

### #27 — Linux M9 gates are operator-run except the 100-child Linux M7 nightly canary

- **Old upstream contract:** phase docs say Phase 3 re-runs Phase 1/2 gates against
  the Linux guest. `determinism-hypervisor/IMPLEMENTATION-PLAN.md` says all
  milestones run on the Intel box and describes CI-required determinism jobs,
  nightly M7-style 100-fork verify, and corpus reverify.
- **Local amendment:** artifact-backed M9 Linux gates depend on staged
  `DH_M9_*` artifacts, live KVM, and deliberate `kvm-intel` scheduling. They are
  operator-run acceptance commands except for the scheduled 100-child Linux M7
  canary in `.github/workflows/nightly-drift.yaml`. Full Linux M7 1000-child
  acceptance and Linux cross-slot rerun are operator-run, not required CI. The
  existing nanokernel/default nightly canary and nanokernel M5 corpus reverify
  remain separate coverage. `*_ALLOW_SKIP=1` evidence is never accepted for M9
  final evidence.
- **Authority:** `docs/ops/test-partitioning.md`; `docs/ops/github-runner.md`;
  `.github/workflows/nightly-drift.yaml`; `docs/phase-1-exit-gate.md`;
  `docs/phase-2-exit-gate.md`; bead 4s9.33.
- **Rollback / follow-up:** making any Linux artifact gate required CI requires
  checked-in or hosted artifacts, deterministic runner provisioning for those
  artifacts, updated fork-PR security policy, and updated gate docs.

### #28 — M9 Linux artifacts and corpus are externally staged; source keeps only the manifest

- **Old upstream contract:** `reference-workload/API.md` describes a
  `WorkloadImage` manifest with attached `bzImage` and `initramfs.cpio.zst`
  artifacts stored by the control-plane registry. `determinism-hypervisor` M5
  language also describes checked-in record/replay corpus bytes that are
  reverified nightly.
- **Local amendment:** this repo does not commit the large M9 Linux kernel,
  initramfs, base image, game image, Linux snapshots, or Linux DHILOG payloads.
  Operators stage them under `$HOME/.cache/dh-m9/reference-workload` and register
  worker artifacts in `DH_M9_IMAGE_CACHE` by lowercase BLAKE3. The Linux M5
  corpus under `crates/dh-worker/tests/fixtures/record_replay_corpus/m9_linux_post_ready/`
  intentionally checks in only `README.md` and `expected.txt`; the test records
  live from staged artifacts and asserts the live DHILOG/snapshot/hash evidence
  matches the manifest.
- **Authority:** `docs/ops/test-partitioning.md`; `docs/ops/github-runner.md`;
  `crates/dh-worker/tests/fixtures/record_replay_corpus/m9_linux_post_ready/README.md`;
  `crates/dh-worker/tests/fixtures/record_replay_corpus/m9_linux_post_ready/expected.txt`;
  `crates/dh-worker/tests/m5_record_replay.rs`; beads 4s9.27 and 4s9.33.
- **Rollback / follow-up:** if Linux artifacts become small or registry-backed
  enough to check in or fetch hermetically, replace the live manifest-only corpus
  with full corpus bytes or a registry-resolved fixture, then update nightly and
  gate docs to reverify it without local operator staging.

### #29 — Linux M5 `m5_net_loopback` uses guest-driven pv-blk I/O, not Linux pv-net

- **Old upstream contract:** Phase 2/M5 coverage includes input-log device-event
  and net-loopback surfaces, and M9 planning asked for Linux M4/M5 frame and pv-net
  regression coverage where applicable.
- **Local amendment:** M9 does not ship a Linux pv-net driver or Linux pv-net gate.
  The Linux `m5_net_loopback` filter is accepted as a guest-driven pv-blk I/O
  loopback: the workload writes, reads, and flushes through the deterministic pv-blk
  path, stamps a meta proof, snapshots BLKO, and VerifyReplay reproduces the sealed
  segment. This is the Linux-equivalent deterministic I/O fixture for M9, not a
  claim that Linux pv-net exists.
- **Authority:** `docs/ops/test-partitioning.md` Linux M5 guest-driven pv-blk
  loopback row; `docs/phase-2-exit-gate.md` M9 Linux rollup;
  `crates/dh-worker/tests/m5_net_loopback.rs`; bead 4s9.28.
- **Rollback / follow-up:** adding Linux pv-net later requires a Linux guest driver,
  deterministic NET_RX/TX contract, DHILOG coverage, snapshot/hash/replay tests,
  and either replacement or explicit parallel coverage for this pv-blk substitute.

---

## Provenance note

The full ledger was reconstructed from the beads Dolt history
(`dolt_history_issues` for `determinism-hypervisor-veu`): the bead's notes field was
overwritten across iterations, so at any one time it only showed the most recent
entries. Entries #1–#7 above were recovered from historical note versions; #8–#10
are the bead's current notes. Local-amendment diffs were re-extracted from this
repo's git history (commits cited per entry) — quote-match against upstream before
applying, in case upstream moved.

Entries #11–#18 were never on the bead at all: they predate it. They were found by
checking whether the pre-`d55ecc3` local doc amendments survived that sync (none
did — `git diff d55ecc3^ d55ecc3 -- .agents/docs/` is the authoritative revert
record, and every old/new pair in that section is quoted verbatim from it). Each
entry's technical claim was re-verified against the current code before inclusion.
