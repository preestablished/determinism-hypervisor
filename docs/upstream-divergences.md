# Upstream planning-tree divergence ledger (bead veu)

`.agents/docs/determinism-hypervisor/{API,ARCHITECTURE,IMPLEMENTATION-PLAN}.md` and
`.agents/docs/guest-sdk/ARCHITECTURE.md` are **synced from an upstream planning
tree** (last sync: commit `d55ecc3`). During implementation this repo found
eighteen places where those documents are stale or wrong; in every case **the code
in this repo is authoritative** (it ships, round-trips, and is pinned by tests).
Five were amended locally after the sync and will be silently reverted by the next
sync unless pushed; five are upstream-only wording fixes (no local doc edit was
needed because the code or a decision doc is the authority); eight more were
amended locally BEFORE the `d55ecc3` sync and were **already silently reverted by
it** — the revert hazard this file exists to prevent is not theoretical, it has
already happened once (entries #11–#18; for those, the current local copies are
stale again too, so applying upstream + re-syncing fixes both sides).

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
- Quick index — amended locally after the sync (exact diffs): #1, #2, #7, #9, #10;
  upstream-only proposals: #3, #4, #5, #6, #8; amended locally before the sync and
  reverted by it (exact diffs recovered from `d55ecc3^`): #11–#18.

---

## Divergences with a local amendment (sync WILL revert these — apply upstream first)

The "New" texts in this section are verbatim copies of review-passed local edits
(commits cited per entry). Once upstream applies an entry and `.agents/docs` is
re-synced, the local amendment is subsumed by the sync; when all eighteen entries
in this file are applied/resolved, bead `veu` can close.

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
  **Local amendment:** this commit (M4 "Perf gates" bullet).
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

---

## Upstream-only wording fixes (no local doc edit; code / decision doc is the authority)

Unlike the section above, the "Proposed new" texts here are newly authored for this
ledger (accurate against the cited code, but not themselves review-passed doc edits)
— upstream is free to rewrap or rephrase as long as the technical content survives.

### #3 — API.md §4: `EVTC` row understates the implemented v1 contents

- **Found:** iteration 65 review. **Authority:** `crates/dh-devices/src/detchannel.rs`
  (`EVTC_LEN = 39`, `EVTC_VERSION = 1`; ships and round-trips, pinned by the
  `evtc_roundtrips_attached_state_and_seqs` test in the same file).
- The row says the section is just the channel base GPA, but EVTC v1 carries:
  `init_lo u32, init_hi u32, init_status u32` (offsets 0/4/8), `inject_iseq`
  flag u8 + u32 (12..17), `last_quiesce_ack` flag u8 + u32 (17..22), then channel
  flag u8 + `gpa u64` + producer seqs `ring_c u32, ring_i u32` (22..39). The ring,
  manifest, and index state genuinely live in guest RAM (that part of the row is
  right); the host-side latch/seq state above does not and must be serialized.

Old (upstream §4):

```
| `EVTC` | detchannel attach state: channel base GPA `u64` (0 = not attached). All ring, manifest, and index state lives in guest RAM and travels with the pages (guest-sdk ARCHITECTURE §2); the host re-attaches at this GPA after restore |
```

Proposed new:

```
| `EVTC` | detchannel host state, 39 bytes (v1): `init_lo u32, init_hi u32, init_status u32`, `inject_iseq` flag u8 + u32, `last_quiesce_ack` flag u8 + u32, attach flag u8 + channel base GPA `u64` + producer seqs `ring_c u32, ring_i u32` (attach flag 0 = not attached). Ring, manifest, and index state lives in guest RAM and travels with the pages (guest-sdk ARCHITECTURE §2); the host re-attaches at the recorded GPA after restore and reinstates the non-reconstructible producer seqs. Authoritative layout: `dh-devices/src/detchannel.rs` (`EVTC_LEN`/`EVTC_VERSION`) |
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
