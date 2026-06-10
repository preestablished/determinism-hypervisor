#!/bin/bash
# Project: determinism-hypervisor
# Change: Phase 2 — Fork & Replay (M4→M7, ends at Platform Milestone 1)
# Generated: 2026-06-10 from docs/prompts/phase-2-of-determinism-hypervisor-fork-and-replay.md
# Normative acceptance source: .agents/docs/determinism-hypervisor/IMPLEMENTATION-PLAN.md M4–M7
#
# Conventions: every bead states host-runnable vs HARDWARE-GATED (kvm-intel
# runner; implementation agents on macOS/aarch64 verify hardware-gated work by
# pushing and reading CI). `bd dep add CHILD PARENT` = child blocked by parent.
#
# RE-RUNNABLE: a 2026-06-10 invocation died at the M6 capture-fixture create
# (40 of 57 beads landed, deps intact). create_bead reuses an existing bead
# with the same exact title instead of duplicating it, and `bd dep add` is
# idempotent, so re-running only fills in what is missing.

set -e

if [ ! -d ".beads" ]; then
    bd init
fi

# Reuse an existing bead (any status) with the same exact title; create otherwise.
find_bead_by_title() {
    bd list --all --limit 0 --json 2>/dev/null \
        | jq -r --arg t "$1" '.[] | select(.title == $t) | .id' | head -n1
}

create_bead() {
    local title="$1"; shift
    local id
    id=$(find_bead_by_title "$title")
    if [ -n "$id" ]; then
        echo "  reuse: $id  $title" >&2
        printf '%s\n' "$id"
    else
        bd create "$title" "$@" --silent
    fi
}

echo "Creating Phase 2 beads..."

# ========================================
# Gate 0: Analysis & cross-repo seams
# ========================================

A_BUILD=$(create_bead "Verify workspace buildability + sibling-repo presence on the execution host" \
  -d "First bead of the phase. cargo check --workspace must load: confirm ../control-plane (determinism-proto), ../guest-sdk (detguest-host, detguest-wire), ../snapshot-store exist as path-dep roots. The planning doc records ../guest-sdk/crates/detguest-host ABSENT on the macOS dev host as of 2026-06-10 (only detguest-agent/detguest-wire/m0-proto-client) - if absent where this runs, file a cross-repo documentation issue and record the workaround (the kvm-intel box has it). Also verify aarch64 cross-check still green (docs/ops/test-partitioning.md has the exact commands). HOST-RUNNABLE." \
  -p 0 -t task -l analysis)

A_STORE=$(create_bead "snapshot-store readiness verification: cargo-check snapstore-client against its API.md surface" \
  -d "Gate for the M4 store-integration track (risk R12). Add ../snapshot-store/crates/snapstore-client as a workspace path dep (same pattern as detguest-host), cargo check, and confirm it exposes the TakeSnapshot/RestoreSnapshot-supporting surface described in .agents/docs/snapshot-store/API.md (gRPC + page channel; the sibling repo's own code/docs are authoritative on divergence - file a doc issue on mismatch). As of 2026-06-10 the client is a 7-line stub: if still absent, HALT the store-integration track (beads that depend on this), file the issue, and let host-runnable M4 work proceed. HOST-RUNNABLE." \
  -p 0 -t task -l analysis)

A_PROTO=$(create_bead "Proto seam decision: real protobuf codegen home for the API.md s2 surface" \
  -d "proto/hypervisor.proto is an empty placeholder; ../control-plane/crates/determinism-proto has hand-written placeholder structs and NO tonic/prost codegen anywhere. Decide: establish codegen in determinism-proto (cross-repo edits to ../control-plane are IN SCOPE for this seam only) vs promote this repo's proto/hypervisor.proto; either way dh-proto re-exports the result. Deliverable: decision doc in docs/decisions/ + skeleton codegen building on both x86_64 and aarch64 (build.rs protoc needs runner provisioning - coordinate with the CI provisioning bead). NO M6 implementation bead may start before this closes. HOST-RUNNABLE." \
  -p 0 -t task -l analysis)

# ========================================
# M4 - snapshot / restore / fork (label m4)
# ========================================

# --- host-runnable codec/canonicalization track ---

M4_DHSNAP=$(create_bead "DHSNAP v1 container codec in dh-snapshot (API.md s4): framing, sections, writer+reader" \
  -d "crates/dh-snapshot is a 9-line stub - greenfield. Implement the API.md s4 container: header, section table, vCPU section (s8.1 doc order - reuse/extend the canonical_vcpu_blob field order from crates/dh-vmm/src/hash.rs rather than inventing a second serialization), device sections (DetDevice::snapshot framing incl. EVTC v1 from detchannel.rs and the empty-serial rule), page sections (full + parent-relative incremental cluster/page sets mirroring blk overlay handling), ENTR. Pure codec: no KVM types in the public API so it builds on aarch64 (follow the bead-v5w gating discipline). HOST-RUNNABLE." \
  -p 0 -t feature -l impl)
bd dep add "$M4_DHSNAP" "$A_BUILD"

M4_GOLDEN_SNAP=$(create_bead "DHSNAP v1.0 golden-bytes fixtures checked in; byte-identical re-serialization asserted" \
  -d "M5-accept item that freezes the format THIS phase: checked-in fixture files + tests asserting decode(encode(x))==x and encode(decode(fixture))==fixture byte-identical. Format freezes at these fixtures (Constraints: settled decisions stay settled). Follow the drift-test discipline (tests/nanokernel/tests/elf_shape.rs pattern). HOST-RUNNABLE." \
  -p 0 -t task -l testing)
bd dep add "$M4_GOLDEN_SNAP" "$M4_DHSNAP"

M4_XSAVE=$(create_bead "XSAVE canonicalization (risk R7): zero clear-XSTATE_BV components on snapshot AND hash paths" \
  -d "New module (dh-snapshot or dh-vmm seam): canonicalize a kvm_xsave region by zeroing every component whose XSTATE_BV bit is clear, so logically-equal state is byte-equal. Wire into BOTH the DHSNAP vCPU section and hash.rs's blob (Phase 1 deferred XSAVE canonicalization to M4 explicitly - hash.rs module docs; note iteration-51 already sources MXCSR from GET_XSAVE because GET_FPU lies). Unit tests with hand-built regions are HOST-RUNNABLE; the live KVM_GET_XSAVE wiring is HARDWARE-GATED. Include the R7 fault-injection test: an UN-canonicalized area must produce a hash difference the test catches." \
  -p 0 -t feature -l impl)
bd dep add "$M4_XSAVE" "$M4_DHSNAP"

M4_ENTR=$(create_bead "ENTR section: entropy {seed, stream, word_pos} round trip through DHSNAP" \
  -d "crates/dh-devices/src/entropy.rs already exposes EntropyState{seed,stream,word_pos} (state()/restore()); encode it as the API.md s4 ENTR section and round-trip it. Mind the word-granularity invariant documented in entropy.rs (sub-word fill remainders are discarded; state() is always word-aligned). HOST-RUNNABLE." \
  -p 1 -t task -l impl)
bd dep add "$M4_ENTR" "$M4_DHSNAP"

# --- hardware-side M4 plumbing ---

M4_VCPU=$(create_bead "Full vCPU capture/restore: KVM GET/SET round trip incl. XSAVE, MSR list, TSC per decision doc" \
  -d "Extend the Phase-1 capture seam (crates/dh-vmm/src/hash.rs canonical_vcpu_blob + msr.rs capture list with the normalized-TSC slot) into a restore-capable codec: KVM_SET_REGS/SREGS/FPU/XSAVE(canonicalized)/VCPU_EVENTS/DEBUGREGS/MSRS. TSC restore MUST follow docs/decisions/tsc-alignment.md (KVM_VCPU_TSC_OFFSET device attr - tsc.rs set_tsc_offset exists; do not reopen). PvClock::set_vns_base + StateHashChain::from_value are the staged seams (docs/phase-1-exit-gate.md lists them). HARDWARE-GATED verification (live GET->SET->GET equality test on the kvm-intel runner)." \
  -p 0 -t feature -l impl)
bd dep add "$M4_VCPU" "$M4_XSAVE"

M4_DIRTY=$(create_bead "Dirty-ring harvest in dh-vmm: per-vCPU ring mmap, harvest API, reset, ring-full handling" \
  -d "KVM_CAP_DIRTY_LOG_RING_ACQ_REL is already required+enabled (kvm.rs KvmCaps, ring size 65536 per ARCH s8.2). Implement: mmap the per-vCPU ring, acquire/rel harvest of dirty GFNs into a page set, KVM_RESET_DIRTY_RINGS, and the ring-FULL path (exit, harvest, resume - must be loss-free by construction). Expose a harvest-at-boundary API for the snapshot engine. ARCH s8.2 + risk R8 (THP already off via MADV_NOHUGEPAGE in kvm.rs memfd setup). HARDWARE-GATED." \
  -p 0 -t feature -l impl)
bd dep add "$M4_DIRTY" "$A_BUILD"

M4_SEAL=$(create_bead "memfd sealing + Frozen slot-state machine (risk R9): F_SEAL_FUTURE_WRITE on frozen parents" \
  -d "Guest RAM is already memfd-backed (kvm.rs create_slot_vm). Add: freeze(slot) applies F_SEAL_FUTURE_WRITE (blocks NEW writable mappings; F_SEAL_WRITE is unavailable while the parent's KVM mapping exists - ARCH s8.4), plus the SOFTWARE-enforced Frozen state in the SlotState machine (lib.rs SlotState exists) guarding the parent's own existing mapping: any write-path API on a Frozen slot is a loud error. Unit-testable state machine HOST-RUNNABLE; seal ioctls HARDWARE-GATED." \
  -p 0 -t feature -l impl)
bd dep add "$M4_SEAL" "$A_BUILD"

M4_TAKE=$(create_bead "Snapshot engine: TakeSnapshot orchestration (pause boundary -> harvest -> DHSNAP -> store)" \
  -d "Pause at a boundary (empty-agenda precondition s8.1), harvest dirty pages (full walk for root, ring-delta for incremental/parent-relative), capture vCPU+devices+ENTR via the DHSNAP codec, push to snapshot-store via snapstore-client (REAL store, never a mock - R12), return the ref only after store durability. Wire StateHashChain continuation. HARDWARE-GATED (joint with the live store)." \
  -p 0 -t feature -l impl)
bd dep add "$M4_TAKE" "$M4_DHSNAP"
bd dep add "$M4_TAKE" "$M4_VCPU"
bd dep add "$M4_TAKE" "$M4_DIRTY"
bd dep add "$M4_TAKE" "$M4_ENTR"
bd dep add "$M4_TAKE" "$A_STORE"

M4_RESTORE=$(create_bead "Restore engine tier-B: mmap restore from store, s8.3 order (RAM -> devices -> vCPU), vns_base" \
  -d "Fetch (root + delta chain) from snapshot-store, materialize RAM (mmap), restore devices in s8.3 order - RAM FIRST then device restore() (detchannel re-attach precondition, EVTC notes in detchannel.rs), then vCPU (M4_VCPU codec), set PvClock vns_base + chain from_value, counter IOC_RESET re-zero (s3.1 icount latch). Target: tier-B warm restore < 150ms p50 (perf bead measures). HARDWARE-GATED." \
  -p 0 -t feature -l impl)
bd dep add "$M4_RESTORE" "$M4_TAKE"

M4_FORK=$(create_bead "Tier-A CoW fork: frozen parent, memfd CoW child RAM, per-slot KVM fds" \
  -d "ARCH s8.4: freeze parent (M4_SEAL), create child slot with CoW view of parent RAM (private mmap of the parent memfd or pagecache-backed copy per s8.4), fresh per-slot KVM fds (no shared kernel state - R9), child inherits vCPU/device state via the restore codec from the parent's in-memory snapshot. Target < 10ms p50 (perf bead measures). HARDWARE-GATED." \
  -p 0 -t feature -l impl)
bd dep add "$M4_FORK" "$M4_SEAL"
bd dep add "$M4_FORK" "$M4_TAKE"
bd dep add "$M4_FORK" "$M4_RESTORE"

M4_PARANOID=$(create_bead "Paranoid-hash audit mode (risk R8): periodic full-memory hash during soak runs" \
  -d "Config/flag that makes run segments hash FULL memory at every epoch (not just final) regardless of hash_epochs, for soak-run auditing of dirty-tracking misses. Cheap bead: the full walk already exists (hash.rs push_final_link). Wire into run config + dh-cli flag. HOST-RUNNABLE logic, HARDWARE-GATED verification." \
  -p 2 -t task -l impl)
bd dep add "$M4_PARANOID" "$M4_TAKE"

# --- M4 acceptance (HARDWARE-GATED; close before any M5 hw acceptance starts) ---

M4_ACC_ROUND=$(create_bead "M4 ACCEPT: snapshot transparency roundtrip - H1 == H2 (1e8+snapshot+restore+1e8 vs 2e8)" \
  -d "IMPLEMENTATION-PLAN M4 accept, tests/determinism: boot landing-loop guest, run 1e8, TakeSnapshot, destroy slot, restore, run 1e8 more -> H1; versus uninterrupted 2e8 -> H2. H1 == H2 exactly (any instruction-count or device-state leak shows here). The critical property of the milestone. HARDWARE-GATED (kvm-intel runner; agents verify via CI)." \
  -p 0 -t task -l testing)
bd dep add "$M4_ACC_ROUND" "$M4_RESTORE"

M4_ACC_FORK=$(create_bead "M4 ACCEPT: fork transparency + frozen-parent second-child reproducibility" \
  -d "Roundtrip test with a tier-A fork in the middle (H1==H2 across the fork); AND parent frozen -> child A diverges with inputs X -> parent's second child B re-run with the same X matches A exactly. tests/determinism. HARDWARE-GATED." \
  -p 0 -t task -l testing)
bd dep add "$M4_ACC_FORK" "$M4_FORK"
bd dep add "$M4_ACC_FORK" "$M4_ACC_ROUND"

M4_ACC_RING=$(create_bead "M4 ACCEPT: dirty-ring-full chaos - ring size 512 forced, hashes unchanged vs large ring" \
  -d "Force ring size 512 so harvest-on-full triggers constantly during the roundtrip test; snapshot hashes must equal the 65536-ring run (R8: misses are caught by construction via H1!=H2). tests/determinism. HARDWARE-GATED." \
  -p 1 -t task -l testing)
bd dep add "$M4_ACC_RING" "$M4_ACC_ROUND"

M4_ACC_ENTR=$(create_bead "M4 ACCEPT: ENTR golden - restore reproduces the next 1024 entropy draws bit-identically" \
  -d "Snapshot mid-stream, restore, draw 1024 more entropy fills: byte-identical to the un-snapshotted continuation ({seed,stream,word_pos} round trip, API.md s4). New nanokernel entropy-draw guest program (or extend device_exercise) drives draws through the real pv-entropy MMIO path. HARDWARE-GATED; the codec round-trip half is host-runnable in M4_ENTR's units." \
  -p 1 -t task -l testing)
bd dep add "$M4_ACC_ENTR" "$M4_RESTORE"

M4_ACC_PERF=$(create_bead "M4 ACCEPT: perf gates p50 on the box, 128MiB guest - fork<10ms, incr snap(8k pages)<15ms, restore<150ms" \
  -d "Criterion benches in a benches/ target: tier-A fork, incremental snapshot at <=8k dirty pages, tier-B warm restore; thresholds are the IMPLEMENTATION-PLAN numbers at p50 on the QUIESCED Intel box (MAP.md canonical 128MiB figure). Wire into the nightly perf-gate job (>20% regression fails) - coordinate with CI beads. HARDWARE-GATED." \
  -p 1 -t task -l testing)
bd dep add "$M4_ACC_PERF" "$M4_FORK"
bd dep add "$M4_ACC_PERF" "$M4_RESTORE"

M4_ACC_STORE=$(create_bead "M4 ACCEPT: joint integration vs the REAL snapshot-store (risk R12) - refs only after durability" \
  -d "TakeSnapshot/RestoreSnapshot round trip against a real snapstore-server instance (never a mock): page round-trip fidelity, parent-relative deltas, ref returned only after store durability ack. Server-on-runner mechanics come from the CI fixture bead. HARDWARE-GATED." \
  -p 0 -t task -l testing)
bd dep add "$M4_ACC_STORE" "$M4_ACC_ROUND"
bd dep add "$M4_ACC_STORE" "$A_STORE"

M4_FASTPATH=$(create_bead "Staged adoption: switch store page channel to the UDS SEQPACKET fast path when store M5 lands" \
  -d "Phase doc staged adoption (.agents/docs/phases/phase-2-fork-and-replay.md + Constraints): integrate via the store gRPC surface FIRST, then switch the page channel to snapshot-store's M5 UDS SEQPACKET fast path with memfd fd-passing when ../snapshot-store ships it. Refs-only-after-durability semantics unchanged; re-run the joint R12 acceptance green on the fast path. If store M5 has not landed when this is claimed, file/track the cross-repo gap and defer - do NOT patch ../snapshot-store (read-only sibling). HARDWARE-GATED." \
  -p 2 -t task -l impl)
bd dep add "$M4_FASTPATH" "$M4_ACC_STORE"

# ========================================
# M5 - DHILOG v1 + replay (label m5)
# ========================================

# --- host-runnable (may start during M4) ---

M5_REPLAY_CODEC=$(create_bead "DHILOG v1 reader/replay iterator + validation in dh-inputlog" \
  -d "crates/dh-inputlog/src/dhilog.rs has the 537-line writer side (records, sealing, watermark). Add the read side: parse header (incl. the encoder fingerprint at 240..248), iterate records with watermark/seq validation, body_hash verification, AUX skipping contract, END semantics. API.md s3 is normative. HOST-RUNNABLE (must build on aarch64)." \
  -p 0 -t feature -l impl)
bd dep add "$M5_REPLAY_CODEC" "$A_BUILD"

M5_CONCAT=$(create_bead "DHILOG concatenation/splicing: child log = parent prefix + segment" \
  -d "Log concatenation for the fork tree: splice (root snapshot, parent log prefix, child segment) into a replayable whole; seq/icount/watermark continuity rules; sealed-log composition semantics per API.md s3. Needed by M7's (root snapshot + spliced log) re-execution. HOST-RUNNABLE." \
  -p 1 -t feature -l impl)
bd dep add "$M5_CONCAT" "$M5_REPLAY_CODEC"

M5_GOLDEN=$(create_bead "DHILOG v1.0 golden-bytes fixtures checked in; byte-identical re-serialization asserted" \
  -d "Same freeze discipline as DHSNAP golden: fixture files covering every record kind (PAD_SET, DEV_EVENT incl. PIO_ANSWER encoding, NET_RX, ENTROPY, TIMER_FIRE, SDK_EVENT, FRAME_MARK, END) + header incl. encoder fingerprint; the v1 format freezes here. HOST-RUNNABLE." \
  -p 0 -t task -l testing)
bd dep add "$M5_GOLDEN" "$M5_REPLAY_CODEC"

M5_FUZZ=$(create_bead "cargo fuzz target for the DHILOG parser + 1h nightly CI job; 24h run as operator dispatch" \
  -d "fuzz/ target over the read path (arbitrary bytes -> parse; no panics/OOM). The M5-accept 24h run does NOT fit an agent bead: model as workflow_dispatch documented for the operator; the recurring artifact is the 1h nightly job (hosted runner is fine - fuzzing is host-runnable; needs cargo-fuzz nightly toolchain - coordinate with provisioning bead). HOST-RUNNABLE." \
  -p 1 -t task -l testing)
bd dep add "$M5_FUZZ" "$M5_REPLAY_CODEC"

M5_PVNET=$(create_bead "pv-net loopback device (ARCH s6.7, MMIO 0xD000_5000) in dh-devices + DHSNAP section" \
  -d "New device: loopback-only net per s6.7 (TX ring -> RX of the same guest; NET_TX digest8 AUX record, NET_RX is a canonical log record - it is INPUT). Follow the established DetDevice patterns (bus 4-byte regs, device-id header, snapshot/restore section, deny-list purity). Plus unit tests mirroring serial/blk style. NET_RX/NET_TX landing has no other producer/consumer - M5 recording depends on this. HOST-RUNNABLE." \
  -p 1 -t feature -l impl)
bd dep add "$M5_PVNET" "$A_BUILD"

M5_GUEST_PAD=$(create_bead "nanokernel pad-echo guest: reads pv-pad latches, echoes to serial/RAM table" \
  -d "Guest for the M5 record/replay acceptance: polls pv-pad latch(es) each fake frame, records observed values to a RAM table + serial, so a scripted 60s-vns pad sequence produces guest-visible state that the hash chain covers. Follow timer_guest/device_exercise patterns (build.rs PROGRAMS, lib.rs accessors, elf_shape pin). HOST-RUNNABLE build." \
  -p 1 -t task -l prep)
bd dep add "$M5_GUEST_PAD" "$A_BUILD"

M5_GUEST_FRAME=$(create_bead "nanokernel fake-frame emitter: FRAME_COUNTER bumps + FRAME_MARK continuity across snapshot/restore" \
  -d "Guest that bumps pv-pad FRAME_COUNTER (s6.4) on a deterministic cadence so FRAME_MARK records populate the frame table; used by the at_frame/frame_budget acceptance INCLUDING across a snapshot/restore (absolute counter carries over). HOST-RUNNABLE build." \
  -p 1 -t task -l prep)
bd dep add "$M5_GUEST_FRAME" "$A_BUILD"

M5_GUEST_NET=$(create_bead "nanokernel net-loopback guest: TX a frame, receive it back, verify payload" \
  -d "Drives pv-net loopback for NET_RX landing tests: writes a known frame to TX, spins for RX delivery, verifies payload, reports via serial. HOST-RUNNABLE build." \
  -p 1 -t task -l prep)
bd dep add "$M5_GUEST_NET" "$M5_PVNET"

# --- hardware-side M5 ---

M5_RECORD=$(create_bead "Recording integration: run loop lands PAD_SET/DEV_EVENT/NET_RX + AUX into DHILOG during live runs" \
  -d "Wire the device-bus run path so every canonical input (PAD_SET applications, detchannel DEV_EVENTs - already logged by DetChannelHost, NET_RX deliveries) and AUX record lands in the segment log at the correct boundary icount, with sealing at segment end (SealParams from the boundary outcome). The m1_acceptance on_exit harness is the wiring template; this productizes it in dh-vmm/runctl or a thin recording layer. HARDWARE-GATED verification." \
  -p 0 -t feature -l impl)
bd dep add "$M5_RECORD" "$M5_REPLAY_CODEC"
bd dep add "$M5_RECORD" "$M5_PVNET"

M5_REPLAY=$(create_bead "Replay path: drive a run from (snapshot, DHILOG) - injections at recorded icounts, bit-identical" \
  -d "Replay executor: restore snapshot, walk the log, apply each canonical record at its icount via the boundary engine (land_at + inject/apply), feed entropy from ENTR state, verify EPOCH_HASH records against the live chain as it goes, end_state_hash at END. This is the product's core property made executable. HARDWARE-GATED." \
  -p 0 -t feature -l impl)
bd dep add "$M5_REPLAY" "$M5_RECORD"
bd dep add "$M5_REPLAY" "$M4_ACC_ROUND"

M5_UNTIL=$(create_bead "runctl: wire Until::NextSdkEvent + Until::FrameBudget (Phase-1 NotYetWired stubs)" \
  -d "crates/dh-vmm/src/runctl.rs has both variants returning NotYetWired. NextSdkEvent needs the device-bus doorbell exits (detchannel drain produced events); FrameBudget needs FRAME_COUNTER exits via pv-pad. Needed by at_frame/frame_budget acceptance and M6 Run RPC semantics (API.md s2.3). HARDWARE-GATED verification." \
  -p 1 -t feature -l impl)
bd dep add "$M5_UNTIL" "$M5_RECORD"

# --- M5 acceptance ---

M5_ACC_RR=$(create_bead "M5 ACCEPT: record/replay 60s-vns scripted pad sequence on pad-echo, x100, every EPOCH_HASH equal" \
  -d "Scripted pad sequence (seeded, 60 guest-seconds of vns) recorded on the pad-echo guest from a snapshot; replay reproduces end_state_hash AND every EPOCH_HASH. Repeat 100x with zero divergence. tests/determinism. HARDWARE-GATED - no M6 hw-acceptance bead starts before this closes." \
  -p 0 -t task -l testing)
bd dep add "$M5_ACC_RR" "$M5_REPLAY"
bd dep add "$M5_ACC_RR" "$M5_GUEST_PAD"

M5_ACC_FRAME=$(create_bead "M5 ACCEPT: at_frame scheduling + frame_budget stops vs FRAME_MARK table, incl. across snapshot/restore" \
  -d "Verify at_frame lands on the absolute FRAME_COUNTER basis (API.md s2.3) using the fake-frame guest's FRAME_MARK table, and frame_budget stops at the right frame - including a snapshot/restore in the middle (absolute counter carries over via the restored device section). HARDWARE-GATED." \
  -p 1 -t task -l testing)
bd dep add "$M5_ACC_FRAME" "$M5_UNTIL"
bd dep add "$M5_ACC_FRAME" "$M5_GUEST_FRAME"
bd dep add "$M5_ACC_FRAME" "$M4_ACC_ROUND"

M5_ACC_NET=$(create_bead "M5 ACCEPT: NET_RX landing recorded + replayed bit-identically on the net-loopback guest" \
  -d "Record a run of the net-loopback guest (TX->loopback->RX with NET_RX canonical records + NET_TX digest AUX); replay reproduces hashes. Covers the only NET producer/consumer until Phase 3. HARDWARE-GATED." \
  -p 1 -t task -l testing)
bd dep add "$M5_ACC_NET" "$M5_REPLAY"
bd dep add "$M5_ACC_NET" "$M5_GUEST_NET"

# ========================================
# M6 - worker daemon + capture (label m6)
# ========================================

M6_PROTO=$(create_bead "Proto schema v1: full API.md s2 surface + tonic/prost codegen + dh-proto re-export" \
  -d "Implement the seam decision (A_PROTO): all API.md s2 messages/RPCs (slot lifecycle, leases, Run with Until/CaptureSpec, TakeSnapshot/RestoreSnapshot/Fork, VerifyReplay, ReadGuestMemory, GetFramebuffer, StreamGuestEvents, WatchSlots, DeterminismClass s2.8) generated and re-exported through crates/dh-proto. Must build on aarch64 (vendored protoc or gated build). HOST-RUNNABLE." \
  -p 0 -t feature -l impl)
bd dep add "$M6_PROTO" "$A_PROTO"

M6_SLOTMGR=$(create_bead "Slot manager + leases in dh-worker: slot table, lease grant/renew/expire, core pinning" \
  -d "ARCH s7 + INTEGRATION leasing protocol: fixed slot set (canonical N_slots from MAP.md), lease tokens gating every mutating RPC, expiry reclaims, slot->core pinning (vCPU threads pin to slot cores 2-5; everything else housekeeping). State machine unit tests HOST-RUNNABLE; pinning/lifecycle HARDWARE-GATED." \
  -p 0 -t feature -l impl)
bd dep add "$M6_SLOTMGR" "$M6_PROTO"
bd dep add "$M6_SLOTMGR" "$M5_ACC_RR"

M6_GRPC=$(create_bead "dh-workerd gRPC service on :7400 + UDS: lifecycle RPCs wired to the real engines" \
  -d "tonic server (TCP :7400 + UDS for the M6 integration test): CreateSlot/RestoreSnapshot/Run/Inject/TakeSnapshot/Fork/DestroySlot/VerifyReplay wired to the M4/M5 engines through the slot manager. dh-workerd grows from the 24-line stub; preflight gate stays the startup precondition. HARDWARE-GATED." \
  -p 0 -t feature -l impl)
bd dep add "$M6_GRPC" "$M6_SLOTMGR"

M6_INTROSPECT=$(create_bead "Introspection RPCs: ReadGuestMemory, GetFramebuffer, StreamGuestEvents" \
  -d "API.md s2: ReadGuestMemory (bounded GPA window reads from a paused slot), GetFramebuffer (capture-engine fed), StreamGuestEvents (drained detchannel GuestEvents as a server stream). HARDWARE-GATED." \
  -p 1 -t feature -l impl)
bd dep add "$M6_INTROSPECT" "$M6_GRPC"

M6_HEALTH=$(create_bead "/healthz + metrics on :7401 with EVERY ARCH s9 series + WatchSlots stream" \
  -d "Prometheus endpoint exposing every series named in ARCH s9 (incl. the existing dh_pmi_skid_instructions histogram shape from dh-verify skid.rs), /healthz wired to preflight status, WatchSlots server-stream of slot state transitions. Metrics-name audit is part of the M6 acceptance. HARDWARE-GATED verification; handler logic host-runnable." \
  -p 1 -t feature -l impl)
bd dep add "$M6_HEALTH" "$M6_GRPC"

M6_CAPTURE=$(create_bead "Capture engine (ARCH s6.10): CaptureSpec on Run/TakeSnapshot, manifest resolution, feature_bytes, lz4 fb" \
  -d "Resolve CaptureSpec against the detchannel region manifest (DetChannelHost::manifest() exists), read flagged regions at the capture boundary, pack feature_bytes, lz4-compress FRAMEBUFFER regions, attach to snapshot/run results. layout_version mismatch -> FAILED_PRECONDITION (C2). Exercised against the REAL guest-sdk manifest in Phase 3; here against the nanokernel capture fixture. HARDWARE-GATED." \
  -p 1 -t feature -l impl)
bd dep add "$M6_CAPTURE" "$M6_GRPC"

M6_GUEST_CAP=$(create_bead "nanokernel M6 capture fixture: minimal region manifest + FRAMEBUFFER region + bumpable layout_version" \
  -d "Guest publishing a detchannel region manifest with one FRAMEBUFFER-flagged region of known content and a cmdline-bumpable layout_version - the only manifest producer until guest-sdk lands in Phase 3 (capture-neutrality C5 and FAILED_PRECONDITION C2 tests need it). Extends the device_exercise channel-init pattern. HOST-RUNNABLE build." \
  -p 1 -t task -l prep)
bd dep add "$M6_GUEST_CAP" "$A_BUILD"

M6_PREFLIGHT=$(create_bead "Preflight extension: dirty-ring cap, memfd sealing support, THP/MADV_NOHUGEPAGE checks" \
  -d "crates/dh-worker/src/preflight.rs gains the M4/M6 host requirements: dirty-ring capability present, memfd F_SEAL_FUTURE_WRITE works (probe on a scratch memfd), THP posture per ARCH s7.4. HARDWARE-GATED (self-skips elsewhere like kvm_checks)." \
  -p 2 -t task -l impl)
bd dep add "$M6_PREFLIGHT" "$M4_SEAL"

M6_ACC_64=$(create_bead "M6 ACCEPT: full-API UDS integration test, 64 concurrent slots, per-slot hashes match single-slot baselines" \
  -d "Drive restore->inject->run->snapshot(CaptureSpec)->destroy over UDS on 64 slots concurrently; per-slot hashes must equal single-slot baselines (catches PMU counter collisions + core-pinning bugs - each vCPU thread needs its own pinned counter per s3.1). HARDWARE-GATED - no M7 hw-acceptance bead starts before M6 acceptance closes." \
  -p 0 -t task -l testing)
bd dep add "$M6_ACC_64" "$M6_GRPC"
bd dep add "$M6_ACC_64" "$M6_INTROSPECT"

M6_ACC_CAP=$(create_bead "M6 ACCEPT: capture-neutrality (C5) + layout_version FAILED_PRECONDITION (C2)" \
  -d "Identical child refs and epoch hashes for capture vs no-capture runs of the same (snapshot, inputs); bumped layout_version fails FAILED_PRECONDITION. Uses the nanokernel capture fixture. HARDWARE-GATED." \
  -p 1 -t task -l testing)
bd dep add "$M6_ACC_CAP" "$M6_CAPTURE"
bd dep add "$M6_ACC_CAP" "$M6_GUEST_CAP"

M6_DOCS=$(create_bead "grpcurl smoke documented + metrics-series audit vs ARCH s9" \
  -d "docs/ops: grpcurl invocations for every RPC (runner needs grpcurl - provisioning bead); audit that every ARCH s9 series exists with the right names/labels (the M6 accept names this explicitly). HARDWARE-GATED checks, doc deliverable." \
  -p 2 -t task -l docs)
bd dep add "$M6_DOCS" "$M6_HEALTH"

# ========================================
# M7 - Platform Milestone 1 (label m7)
# ========================================

M7_VERIFY=$(create_bead "VerifyReplay execution path in dh-verify: epoch-hash comparison, VerifyDone/Divergence reporting" \
  -d "Execute a (snapshot, log) pair and compare every EPOCH_HASH + end_state_hash against the recorded values: VerifyDone on full match, Divergence{first_divergent_epoch, hashes} otherwise (INTEGRATION VerifyReplay model; bisection refinement is M8 - OUT of scope). Exposed both as a library harness (gate.rs style) and the M6 RPC. HARDWARE-GATED." \
  -p 0 -t feature -l impl)
bd dep add "$M7_VERIFY" "$M5_REPLAY"

M7_HARNESS=$(create_bead "M7 ACCEPT: fork 1000x harness - root snapshot, 1000 seeded pad-burst children, VerifyReplay all" \
  -d "Platform Milestone 1 (MAP.md step 1): boot -> root snapshot -> 1000 tier-A forks batched across slots; each child runs a DISTINCT seeded 1-guest-second random pad burst, TakeSnapshot each; VerifyReplay every (snapshot, spliced log) -> 1000/1000 VerifyDone with matching end_state_hash, zero Divergence. HARDWARE-GATED; this is the phase exit." \
  -p 0 -t task -l testing)
bd dep add "$M7_HARNESS" "$M7_VERIFY"
bd dep add "$M7_HARNESS" "$M6_ACC_64"
bd dep add "$M7_HARNESS" "$M5_CONCAT"
bd dep add "$M7_HARNESS" "$M4_ACC_FORK"
bd dep add "$M7_HARNESS" "$M4_ACC_STORE"

M7_XCHECK=$(create_bead "M7 ACCEPT: determinism cross-check - 10 of 1000 jobs re-run from root on a DIFFERENT slot, identical refs" \
  -d "Re-run 10 of the 1000 jobs from the root snapshot on a different slot: content-addressed child snapshot refs must be identical (bit-identical re-execution across slots). HARDWARE-GATED." \
  -p 1 -t task -l testing)
bd dep add "$M7_XCHECK" "$M7_HARNESS"

M7_SOAK=$(create_bead "M7 ACCEPT: throughput soak - >= N_slots x 1 job/s for 30min under stress-ng housekeeping load" \
  -d "Sustained N_slots x 1 job/s (N_slots = MAP.md canonical slot count) for >= 30 min with stress-ng pinned to housekeeping cores; exits/PMI/hashes unaffected by load - HASHES are the assertion. Within ARCH s10 per-job budget. HARDWARE-GATED (runner needs stress-ng - provisioning bead)." \
  -p 1 -t task -l testing)
bd dep add "$M7_SOAK" "$M7_HARNESS"

M7_BUDGET=$(create_bead "M7 ACCEPT: exit-gate storage budget - end-to-end exploration step <= 100ms p50" \
  -d "Phase 2 exit gate item 2 (.agents/docs/phases/phase-2-fork-and-replay.md): the JOINT exploration-step storage budget <= 100ms p50, verified end-to-end on the quiesced box - fork -> run -> TakeSnapshot -> store durability ack measured as one step. Hypervisor-side fork<10ms is gated by the M4 perf bead and the store's 8ms delta-commit budget is store-side; this bead owns the end-to-end number. Measure during/alongside the M7 harness runs. HARDWARE-GATED." \
  -p 1 -t task -l testing)
bd dep add "$M7_BUDGET" "$M7_HARNESS"

M7_CLI=$(create_bead "dh-cli operator verbs: snapshot / restore / fork / replay / verify debugging commands" \
  -d "Extend tools/dh-cli (cli.rs subcommand pattern) with operator verbs over the new engines: take/restore a snapshot to/from the store, fork a slot, replay a (snapshot, log) pair, verify. Debug aid for everything above; JSON output like boot --json. HARDWARE-GATED runtime, host-buildable." \
  -p 2 -t task -l impl)
bd dep add "$M7_CLI" "$M5_REPLAY"

# ========================================
# Milestone acceptance barriers
# (Constraints: no bead of milestone N+1's hardware-gated acceptance may
# start before milestone N's acceptance beads close. bd dep add is
# idempotent; redundant-with-transitive edges are harmless.)
# ========================================

# M4 acceptance (6 beads) gates all M5 hw acceptance
for parent in "$M4_ACC_ROUND" "$M4_ACC_FORK" "$M4_ACC_RING" "$M4_ACC_ENTR" "$M4_ACC_PERF" "$M4_ACC_STORE"; do
    bd dep add "$M5_ACC_RR" "$parent"
    bd dep add "$M5_ACC_FRAME" "$parent"
    bd dep add "$M5_ACC_NET" "$parent"
done

# M5 acceptance (record/replay, frame, net + the golden/fuzz freeze items the
# IMPLEMENTATION-PLAN lists under M5 accept) gates all M6 hw acceptance
for parent in "$M5_ACC_RR" "$M5_ACC_FRAME" "$M5_ACC_NET" "$M5_GOLDEN" "$M4_GOLDEN_SNAP" "$M5_FUZZ"; do
    bd dep add "$M6_ACC_64" "$parent"
    bd dep add "$M6_ACC_CAP" "$parent"
done

# M6 acceptance (64-slot, capture, grpcurl/metrics audit) gates M7 acceptance
# (M7_XCHECK/M7_SOAK/M7_BUDGET are gated transitively via M7_HARNESS)
for parent in "$M6_ACC_64" "$M6_ACC_CAP" "$M6_DOCS"; do
    bd dep add "$M7_HARNESS" "$parent"
done

# ========================================
# CI / chaos / corpus (label determinism-ci)
# ========================================

CI_CHECKOUT=$(create_bead "CI: add snapshot-store sibling checkout to both lanes + runner tool provisioning" \
  -d "ci.yaml + nightly-drift.yaml checkout steps gain ../snapshot-store (third sibling); provision + document in docs/ops/github-runner.md: protoc (tonic codegen), grpcurl (M6 smoke), stress-ng (M7 soak), cargo-fuzz + nightly toolchain (M5 fuzz). Hosted lanes need the checkout too (path dep resolves at cargo metadata). HOST-RUNNABLE edits, runner provisioning is ops." \
  -p 1 -t task -l prep)
bd dep add "$CI_CHECKOUT" "$A_STORE"

CI_STORE_FIXTURE=$(create_bead "Decision+implementation: real snapstore-server on the kvm-intel runner (build-and-spawn vs provisioned service)" \
  -d "R12 joint tests need a REAL store. Decide: test fixture that builds ../snapshot-store and spawns snapstore-server per test run (hermetic, slower) vs a provisioned long-running service on the box (fast, drift risk). Document in docs/ops + implement the chosen mechanism for tests/determinism + CI. HARDWARE-GATED." \
  -p 1 -t task -l prep)
bd dep add "$CI_STORE_FIXTURE" "$A_STORE"
bd dep add "$M4_ACC_STORE" "$CI_STORE_FIXTURE"

CI_NIGHTLY_FORK=$(create_bead "CI: nightly M7-style 100-fork verify job" \
  -d "Scaled-down M7 (100 forks) as a nightly job on the kvm-intel runner, failure routed to the existing nightly-drift alert issue pattern. Testing-strategy table row (b). HARDWARE-GATED." \
  -p 1 -t task -l testing)
bd dep add "$CI_NIGHTLY_FORK" "$M7_HARNESS"

CI_CORPUS=$(create_bead "Record/replay corpus: checked-in (root image, DHILOG, expected hashes) set + nightly re-verification" \
  -d "Testing-strategy row (c): a corpus of recorded pairs with expected chained hashes, re-verified nightly against ci/determinism-class.lock so ANY behavioral drift (code, kernel, microcode) is caught with a named culprit. A class-lock bump re-baselines the corpus (the documented procedure). HARDWARE-GATED nightly; corpus files host-storable." \
  -p 1 -t task -l testing)
bd dep add "$CI_CORPUS" "$M5_ACC_RR"

CI_PERF=$(create_bead "CI: criterion perf-gate nightly - fork/snapshot/restore/landing benches, >20% regression fails" \
  -d "Wire the M4 perf benches + a landing-latency bench into a nightly criterion job on the quiesced runner; baseline storage + >20% regression failure; failure routed to the alert issue. HARDWARE-GATED." \
  -p 1 -t task -l testing)
bd dep add "$CI_PERF" "$M4_ACC_PERF"

CI_CHAOS=$(create_bead "CI: nightly chaos jobs - host load, tiny dirty rings, PMI storms, store latency injection" \
  -d "Testing-strategy chaos row: stress-ng host load during the determinism battery, ring-512 runs, PMI-storm schedule (dense landing targets), snapshot-store latency injection (fixture-level delay shim - the store itself is read-only to us). Hashes are the assertion everywhere. HARDWARE-GATED." \
  -p 2 -t task -l testing)
bd dep add "$CI_CHAOS" "$M4_ACC_RING"
bd dep add "$CI_CHAOS" "$M4_ACC_STORE"

DOCS_ASBUILT=$(create_bead "Phase-2 as-built docs: snapshot/fork/replay architecture notes, ops runbooks, exit-gate record" \
  -d "Mirror the Phase-1 close-out: as-built sections (engines, formats frozen at golden fixtures, measured perf numbers), docs/ops updates (store fixture, new tools), and the Phase-2 exit-gate sign-off record (re-run evidence, scope split vs sibling repos) following docs/phase-1-exit-gate.md's format. HOST-RUNNABLE." \
  -p 2 -t task -l docs)
bd dep add "$DOCS_ASBUILT" "$M7_HARNESS"

echo ""
echo "Phase 2 bead graph created. View with:"
echo "  bd ready   # should show the three gate-0 analysis beads"
