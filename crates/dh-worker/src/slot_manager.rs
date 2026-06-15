//! Slot manager (bead ol1): the ARCH §9 worker model's slot table and
//! the INTEGRATION §2 leasing protocol — a fixed slot set, lease tokens
//! gating every mutating RPC, optional expiry reclaims, and the
//! slot→core pinning map (vCPU threads pin to the §7.4 slot cores;
//! everything else stays on housekeeping cores).
//!
//! SHAPE: the manager is PURE BOOKKEEPING over `dh_vmm::SlotState` — it
//! owns no KVM resources. The daemon (gRPC service, bead rfv) holds the
//! actual `SlotVm`s and engines; every state crossing and every lease
//! check goes through here so the state machine has exactly one home.
//! All transitions delegate to `SlotState::can_transition` (single
//! source of truth) and every guest-mutating entry point composes
//! `SlotState::ensure_write_path` (the R9 guard's documented adoption
//! point — see dh-vmm lib.rs "INTEGRATION (not yet wired)").
//!
//! LEASES (INTEGRATION §2): `CreateVm`/`RestoreSnapshot`/`Fork` return
//! `Lease{slot_id, token}` (16 random bytes); every mutating RPC echoes
//! it and a stale token maps to `FAILED_PRECONDITION`. v1 leases have NO
//! timeout (trusted single orchestrator) — `LeasePolicy::default()`
//! encodes that. The bead's grant/renew/expire mechanics are still
//! implemented behind `LeasePolicy::with_ttl`, so turning expiry on is a
//! config change, not a code change; `reclaim_expired` is the reaper.
//!
//! TIME: expiry needs a clock, and host wall-clock must never leak into
//! guest-visible state — so the manager never reads one. Callers pass
//! `now_ms` explicitly (the daemon's housekeeping loop owns the read),
//! which also makes every unit test deterministic.
//!
//! FORK ACCOUNTING (R9): `fork` freezes the parent (Paused → Frozen)
//! and registers each tier-A CoW child as Paused with
//! `ram_is_cow = true`, counted against the parent's `live_children`.
//! Destroying the last child auto-thaws the parent (Frozen → Paused,
//! the §2.2 DestroyVm semantics). A Frozen parent with live children
//! refuses ordinary destroy — its memfd is the children's shared
//! baseline; `force_destroy` (host-integrity path: seal read error, KVM
//! fd death) cascades by marking every live child Faulted first.

use dh_vmm::{SlotState, SlotStateError};
use std::collections::HashSet;
use std::sync::Mutex;
use tokio::sync::broadcast;

/// 16 random bytes, the proto `Lease.token` payload.
pub type LeaseToken = [u8; 16];

/// The lease handed to the orchestrator (proto `Lease` mirror; the wire
/// crossing itself lives in `proto_map`, never here).
#[derive(Clone, PartialEq, Eq)]
pub struct Lease {
    pub slot_id: u64,
    pub token: LeaseToken,
}

/// Manual Debug: the token gates every mutating RPC, so a `{lease:?}`
/// at some future daemon log site must not leak it.
impl std::fmt::Debug for Lease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lease")
            .field("slot_id", &self.slot_id)
            .field("token", &"[redacted; 16]")
            .finish()
    }
}

/// Expiry policy. v1 default: no timeout (INTEGRATION §2 — trusted
/// single orchestrator; `DestroyVm` releases).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LeasePolicy {
    /// `None` = leases never expire. `Some(ttl)` = a lease not renewed
    /// within `ttl` ms of its grant/renewal is reclaimable.
    pub ttl_ms: Option<u64>,
}

impl LeasePolicy {
    pub fn with_ttl(ttl_ms: u64) -> Self {
        LeasePolicy {
            ttl_ms: Some(ttl_ms),
        }
    }
}

/// The §2.8 introspection row (proto `SlotInfo` mirror).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotInfo {
    pub slot_id: u64,
    pub state: SlotState,
    pub icount: u64,
    /// Base snapshot of the current segment, if any (32-byte ref id).
    pub base_snapshot_id: Option<[u8; 32]>,
    /// Live tier-A CoW children (Frozen parents only).
    pub live_children: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SlotError {
    NoSuchSlot(u64),
    /// Slot-state machine violation — maps to `FAILED_PRECONDITION`.
    State(SlotStateError),
    /// Token absent or mismatched — `FAILED_PRECONDITION` (API.md §2.9
    /// "wrong slot state / stale lease").
    StaleLease {
        slot_id: u64,
    },
    /// Token matched but its TTL elapsed — also `FAILED_PRECONDITION`;
    /// distinct variant so reclaim races diagnose cleanly.
    LeaseExpired {
        slot_id: u64,
    },
    /// R9: the Frozen parent's memfd is its children's shared baseline.
    LiveChildren {
        slot_id: u64,
        children: u32,
    },
    /// fork_engine contract: a tier-A CoW child's RAM cannot be frozen
    /// or re-forked (snapshot → restore instead) — `FAILED_PRECONDITION`.
    CowChildCannotFork {
        slot_id: u64,
    },
    /// A zero-child fork would freeze the parent with nothing to ever
    /// thaw it — `INVALID_ARGUMENT`.
    ZeroChildFork {
        slot_id: u64,
    },
    /// Every slot is occupied — `RESOURCE_EXHAUSTED`.
    NoFreeSlot,
    /// Slot count exceeds the dedicated-core supply (ARCH §9: each slot
    /// gets its own isolated core).
    NotEnoughCores {
        slots: usize,
        cores: usize,
    },
    /// Core map assigns the same physical core to more than one slot.
    DuplicateCore {
        core: u32,
    },
}

impl From<SlotStateError> for SlotError {
    fn from(e: SlotStateError) -> Self {
        SlotError::State(e)
    }
}

#[derive(Clone, Debug)]
struct SlotEntry {
    state: SlotState,
    /// `(token, expires_at_ms)`; `expires_at_ms = None` under the v1
    /// no-timeout policy.
    lease: Option<(LeaseToken, Option<u64>)>,
    live_children: u32,
    /// Set on tier-A CoW children: the slot whose memfd backs our RAM.
    parent: Option<u64>,
    /// Tier-A CoW child RAM cannot be frozen/re-forked (fork_engine
    /// contract) — tracked here so `fork` can refuse early.
    ram_is_cow: bool,
    icount: u64,
    base_snapshot_id: Option<[u8; 32]>,
}

impl SlotEntry {
    fn empty() -> Self {
        SlotEntry {
            state: SlotState::Empty,
            lease: None,
            live_children: 0,
            parent: None,
            ram_is_cow: false,
            icount: 0,
            base_snapshot_id: None,
        }
    }
}

type TokenSource = Box<dyn Fn() -> LeaseToken + Send + Sync>;

/// The slot table. One mutex over the whole table: every operation is a
/// handful of field updates (the engines' heavy work happens outside,
/// in the daemon), so finer locking would buy contention bugs, not
/// throughput.
pub struct SlotManager {
    slots: Mutex<Vec<SlotEntry>>,
    cores: Vec<u32>,
    policy: LeasePolicy,
    tokens: TokenSource,
    events: broadcast::Sender<SlotInfo>,
}

/// ARCH §9 default slot count: `physical_cores - 2` (the two reserved
/// housekeeping cores), floored at 1.
pub fn default_slot_count(physical_cores: usize) -> usize {
    physical_cores.saturating_sub(2).max(1)
}

/// Parse a kernel-style core list ("2-5", "2,4-5") into core ids —
/// the `preflight::SLOT_CORES` format.
pub fn parse_core_list(spec: &str) -> Option<Vec<u32>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b): (u32, u32) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
                if a > b {
                    return None;
                }
                for core in a..=b {
                    if !seen.insert(core) {
                        return None;
                    }
                    out.push(core);
                }
            }
            None => {
                let core = part.parse().ok()?;
                if !seen.insert(core) {
                    return None;
                }
                out.push(core);
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

impl SlotManager {
    /// `cores` are the isolated slot cores in slot order; ARCH §9 gives
    /// every slot a DEDICATED core, so `n_slots > cores.len()` refuses.
    pub fn new(n_slots: usize, cores: Vec<u32>, policy: LeasePolicy) -> Result<Self, SlotError> {
        Self::with_token_source(n_slots, cores, policy, Box::new(urandom_token))
    }

    /// Deterministic-token seam for tests (and any future audit mode).
    pub fn with_token_source(
        n_slots: usize,
        cores: Vec<u32>,
        policy: LeasePolicy,
        tokens: TokenSource,
    ) -> Result<Self, SlotError> {
        let mut seen = HashSet::new();
        for &core in &cores {
            if !seen.insert(core) {
                return Err(SlotError::DuplicateCore { core });
            }
        }
        if n_slots > cores.len() {
            return Err(SlotError::NotEnoughCores {
                slots: n_slots,
                cores: cores.len(),
            });
        }
        Ok(SlotManager {
            slots: Mutex::new(vec![SlotEntry::empty(); n_slots]),
            cores,
            policy,
            tokens,
            events: broadcast::channel(1024).0,
        })
    }

    pub fn slot_count(&self) -> usize {
        self.slots.lock().expect("slot table poisoned").len()
    }

    /// The dedicated core for a slot's vCPU thread.
    pub fn core_for(&self, slot_id: u64) -> Option<u32> {
        self.cores.get(slot_id as usize).copied()
    }

    /// Subscribe to slot transition events. `ListSlots` is the current-state
    /// source; this stream is only for subsequent state transitions.
    pub fn subscribe(&self) -> broadcast::Receiver<SlotInfo> {
        self.events.subscribe()
    }

    fn publish(&self, events: impl IntoIterator<Item = SlotInfo>) {
        for event in events {
            let _ = self.events.send(event);
        }
    }

    // ── Lease mechanics ──────────────────────────────────────────────────

    fn mint(&self, now_ms: u64) -> (LeaseToken, Option<u64>) {
        let token = (self.tokens)();
        (
            token,
            self.policy.ttl_ms.map(|ttl| now_ms.saturating_add(ttl)),
        )
    }

    /// Token + expiry check for `lease` against its slot. Every mutating
    /// path goes through here — the INTEGRATION §2 stale-token gate.
    pub fn validate(&self, lease: &Lease, now_ms: u64) -> Result<(), SlotError> {
        let slots = self.slots.lock().expect("slot table poisoned");
        let entry = slots
            .get(lease.slot_id as usize)
            .ok_or(SlotError::NoSuchSlot(lease.slot_id))?;
        Self::validate_entry(entry, lease, now_ms)
    }

    fn validate_entry(entry: &SlotEntry, lease: &Lease, now_ms: u64) -> Result<(), SlotError> {
        match &entry.lease {
            Some((token, expires)) if *token == lease.token => match expires {
                Some(at) if now_ms >= *at => Err(SlotError::LeaseExpired {
                    slot_id: lease.slot_id,
                }),
                _ => Ok(()),
            },
            _ => Err(SlotError::StaleLease {
                slot_id: lease.slot_id,
            }),
        }
    }

    /// `validate` + the R9 write-path guard, for every API that can
    /// mutate guest RAM / vCPU / device state (run, inject,
    /// restore-into, MMIO dispatch). `api` names the caller for the
    /// loud error.
    ///
    /// TOCTOU contract: the `Ok` is a point-in-time check — no lock is
    /// held while the caller drives the engines. Safe under the v1
    /// single-orchestrator no-timeout policy; the day a TTL policy is
    /// enabled, the daemon must re-validate after any await/suspension
    /// and serialize engine work per slot.
    pub fn checkout_write(
        &self,
        lease: &Lease,
        api: &'static str,
        now_ms: u64,
    ) -> Result<(), SlotError> {
        let slots = self.slots.lock().expect("slot table poisoned");
        let entry = slots
            .get(lease.slot_id as usize)
            .ok_or(SlotError::NoSuchSlot(lease.slot_id))?;
        Self::validate_entry(entry, lease, now_ms)?;
        entry.state.ensure_write_path(api)?;
        Ok(())
    }

    /// Extend a live lease by the policy TTL. A no-op without a TTL; a
    /// stale or already-expired lease is NOT resurrected.
    pub fn renew(&self, lease: &Lease, now_ms: u64) -> Result<(), SlotError> {
        let mut slots = self.slots.lock().expect("slot table poisoned");
        let entry = slots
            .get_mut(lease.slot_id as usize)
            .ok_or(SlotError::NoSuchSlot(lease.slot_id))?;
        Self::validate_entry(entry, lease, now_ms)?;
        if let (Some(ttl), Some((_, expires))) = (self.policy.ttl_ms, entry.lease.as_mut()) {
            *expires = Some(now_ms.saturating_add(ttl));
        }
        Ok(())
    }

    // ── Lifecycle ────────────────────────────────────────────────────────

    /// Claim the first Empty slot (CreateVm / RestoreSnapshot): the slot
    /// lands Paused with a fresh lease. The caller then drives the
    /// engines; a failure there must `destroy` to return the slot.
    pub fn allocate(&self, now_ms: u64) -> Result<Lease, SlotError> {
        let (lease, event) = {
            let mut slots = self.slots.lock().expect("slot table poisoned");
            let idx = slots
                .iter()
                .position(|e| e.state == SlotState::Empty)
                .ok_or(SlotError::NoFreeSlot)?;
            slots[idx].state = slots[idx].state.transition(SlotState::Paused)?;
            let lease = self.lease_into(&mut slots[idx], idx as u64, now_ms);
            (lease, Self::info_of(&slots[idx], idx as u64))
        };
        self.publish([event]);
        Ok(lease)
    }

    fn lease_into(&self, entry: &mut SlotEntry, slot_id: u64, now_ms: u64) -> Lease {
        let (token, expires) = self.mint(now_ms);
        entry.lease = Some((token, expires));
        Lease { slot_id, token }
    }

    /// Fork (§8.4): freeze the Paused parent and register `children`
    /// tier-A CoW children, each Paused with `ram_is_cow = true` and a
    /// fresh lease, all accounted against the parent. All-or-nothing:
    /// without enough Empty slots, nothing changes (RESOURCE_EXHAUSTED).
    /// A CoW child refuses to be a fork parent (fork_engine contract:
    /// snapshot → restore instead), and a zero-child fork is refused
    /// outright — it would freeze the parent with nothing to ever thaw
    /// it (the only exit from childless-Frozen is Destroy).
    pub fn fork(
        &self,
        parent: &Lease,
        children: usize,
        now_ms: u64,
    ) -> Result<Vec<Lease>, SlotError> {
        let (leases, events) = {
            let mut slots = self.slots.lock().expect("slot table poisoned");
            let (parent_idx, frozen, free) =
                Self::check_fork_entries(&slots, parent, children, now_ms)?;
            slots[parent_idx].state = frozen;
            let parent_position = (slots[parent_idx].icount, slots[parent_idx].base_snapshot_id);
            let mut leases = Vec::with_capacity(children);
            let mut events = Vec::with_capacity(children + 1);
            for idx in free {
                let entry = &mut slots[idx];
                entry.state = entry
                    .state
                    .transition(SlotState::Paused)
                    .expect("Empty -> Paused is always legal");
                entry.parent = Some(parent.slot_id);
                entry.ram_is_cow = true;
                // Children resume at the parent's position (fork_engine's
                // cumulative_icount contract).
                entry.icount = parent_position.0;
                entry.base_snapshot_id = parent_position.1;
                let lease = self.lease_into(entry, idx as u64, now_ms);
                events.push(Self::info_of(entry, idx as u64));
                leases.push(lease);
            }
            // Bounded by the free-slot count (≤ slot table length), so the
            // narrowing is total; try_from keeps it honest.
            slots[parent_idx].live_children = slots[parent_idx]
                .live_children
                .saturating_add(u32::try_from(children).expect("children ≤ slot count"));
            events.push(Self::info_of(&slots[parent_idx], parent.slot_id));
            (leases, events)
        };
        self.publish(events);
        Ok(leases)
    }

    /// Validate that `fork` would be accepted without freezing the
    /// parent or claiming child slots. The daemon uses this before
    /// consulting runtime resources so lease/state errors come from the
    /// slot-manager authority first.
    pub fn check_fork(
        &self,
        parent: &Lease,
        children: usize,
        now_ms: u64,
    ) -> Result<(), SlotError> {
        let slots = self.slots.lock().expect("slot table poisoned");
        Self::check_fork_entries(&slots, parent, children, now_ms).map(|_| ())
    }

    fn check_fork_entries(
        slots: &[SlotEntry],
        parent: &Lease,
        children: usize,
        now_ms: u64,
    ) -> Result<(usize, SlotState, Vec<usize>), SlotError> {
        let parent_idx = parent.slot_id as usize;
        if parent_idx >= slots.len() {
            return Err(SlotError::NoSuchSlot(parent.slot_id));
        }
        Self::validate_entry(&slots[parent_idx], parent, now_ms)?;
        if children == 0 {
            return Err(SlotError::ZeroChildFork {
                slot_id: parent.slot_id,
            });
        }
        if slots[parent_idx].ram_is_cow {
            return Err(SlotError::CowChildCannotFork {
                slot_id: parent.slot_id,
            });
        }
        // Transition check BEFORE claiming child slots (all-or-nothing).
        let frozen = slots[parent_idx].state.transition(SlotState::Frozen)?;
        let free: Vec<usize> = slots
            .iter()
            .enumerate()
            .filter(|(_, e)| e.state == SlotState::Empty)
            .map(|(i, _)| i)
            .take(children)
            .collect();
        if free.len() < children {
            return Err(SlotError::NoFreeSlot);
        }
        Ok((parent_idx, frozen, free))
    }

    /// Run / Pause crossings (lease-gated; the run side is write-path
    /// guarded — a Frozen or Faulted slot must never enter the guest).
    pub fn mark_running(&self, lease: &Lease, now_ms: u64) -> Result<(), SlotError> {
        let event = {
            let mut slots = self.slots.lock().expect("slot table poisoned");
            let entry = slots
                .get_mut(lease.slot_id as usize)
                .ok_or(SlotError::NoSuchSlot(lease.slot_id))?;
            Self::validate_entry(entry, lease, now_ms)?;
            entry.state.ensure_write_path("Run")?;
            entry.state = entry.state.transition(SlotState::Running)?;
            Self::info_of(entry, lease.slot_id)
        };
        self.publish([event]);
        Ok(())
    }

    pub fn mark_paused(&self, lease: &Lease, now_ms: u64) -> Result<(), SlotError> {
        let event = {
            let mut slots = self.slots.lock().expect("slot table poisoned");
            let entry = slots
                .get_mut(lease.slot_id as usize)
                .ok_or(SlotError::NoSuchSlot(lease.slot_id))?;
            Self::validate_entry(entry, lease, now_ms)?;
            entry.state = entry.state.transition(SlotState::Paused)?;
            Self::info_of(entry, lease.slot_id)
        };
        self.publish([event]);
        Ok(())
    }

    /// Run completion crossing: publish Paused together with the final
    /// deterministic position so WatchSlots observers do not see a stale
    /// boundary for the transition that matters most.
    pub fn mark_paused_at_position(
        &self,
        lease: &Lease,
        icount: u64,
        base_snapshot_id: Option<[u8; 32]>,
        now_ms: u64,
    ) -> Result<(), SlotError> {
        let event = {
            let mut slots = self.slots.lock().expect("slot table poisoned");
            let entry = slots
                .get_mut(lease.slot_id as usize)
                .ok_or(SlotError::NoSuchSlot(lease.slot_id))?;
            Self::validate_entry(entry, lease, now_ms)?;
            entry.icount = icount;
            entry.base_snapshot_id = base_snapshot_id;
            entry.state = entry.state.transition(SlotState::Paused)?;
            Self::info_of(entry, lease.slot_id)
        };
        self.publish([event]);
        Ok(())
    }

    /// Record the slot's deterministic position (boundary icount + the
    /// segment's base snapshot) for §2.8 introspection.
    pub fn set_position(
        &self,
        lease: &Lease,
        icount: u64,
        base_snapshot_id: Option<[u8; 32]>,
        now_ms: u64,
    ) -> Result<(), SlotError> {
        let mut slots = self.slots.lock().expect("slot table poisoned");
        let entry = slots
            .get_mut(lease.slot_id as usize)
            .ok_or(SlotError::NoSuchSlot(lease.slot_id))?;
        Self::validate_entry(entry, lease, now_ms)?;
        entry.icount = icount;
        entry.base_snapshot_id = base_snapshot_id;
        Ok(())
    }

    /// Fault crossing — deliberately NOT lease-gated: faults are the
    /// worker's own observations (divergence, log DATA_LOSS, counter
    /// revocation) and must land even when the orchestrator's lease has
    /// expired mid-run.
    pub fn mark_faulted(&self, slot_id: u64) -> Result<(), SlotError> {
        let event = {
            let mut slots = self.slots.lock().expect("slot table poisoned");
            let entry = slots
                .get_mut(slot_id as usize)
                .ok_or(SlotError::NoSuchSlot(slot_id))?;
            entry.state = entry.state.transition(SlotState::Faulted)?;
            Self::info_of(entry, slot_id)
        };
        self.publish([event]);
        Ok(())
    }

    /// DestroyVm: release the slot. Refuses a Frozen parent with live
    /// children (R9 — the memfd is their baseline) and a Running slot
    /// (no deterministic boundary; pause first — the transition relation
    /// enforces that). Destroying the last child auto-thaws its parent.
    pub fn destroy(&self, lease: &Lease, now_ms: u64) -> Result<(), SlotError> {
        let events = {
            let mut slots = self.slots.lock().expect("slot table poisoned");
            let idx = lease.slot_id as usize;
            if idx >= slots.len() {
                return Err(SlotError::NoSuchSlot(lease.slot_id));
            }
            Self::check_destroy_entry(&slots[idx], lease, now_ms)?;
            let mut events = Vec::new();
            Self::release(&mut slots, idx, &mut events);
            events
        };
        self.publish(events);
        Ok(())
    }

    /// Validate that `destroy` would be accepted without publishing the
    /// slot as Empty. The daemon uses this before removing runtime
    /// resources so runtime teardown and slot-manager release cannot
    /// diverge.
    pub fn check_destroy(&self, lease: &Lease, now_ms: u64) -> Result<(), SlotError> {
        let slots = self.slots.lock().expect("slot table poisoned");
        let idx = lease.slot_id as usize;
        if idx >= slots.len() {
            return Err(SlotError::NoSuchSlot(lease.slot_id));
        }
        Self::check_destroy_entry(&slots[idx], lease, now_ms)
    }

    fn check_destroy_entry(entry: &SlotEntry, lease: &Lease, now_ms: u64) -> Result<(), SlotError> {
        Self::validate_entry(entry, lease, now_ms)?;
        if entry.live_children > 0 {
            return Err(SlotError::LiveChildren {
                slot_id: lease.slot_id,
                children: entry.live_children,
            });
        }
        // Gate-only: destroy() later resets the whole entry to Empty.
        entry.state.transition(SlotState::Empty)?;
        Ok(())
    }

    /// Host-integrity destroy (seal read error, KVM fd death) — no
    /// lease, and a Frozen parent cascades: every live child's shared
    /// baseline just became untrustworthy, so each is marked Faulted
    /// (never silently destroyed — the orchestrator must observe the
    /// fault). Running slots still refuse: stop the vCPU first.
    pub fn force_destroy(&self, slot_id: u64) -> Result<(), SlotError> {
        let events = {
            let mut slots = self.slots.lock().expect("slot table poisoned");
            let idx = slot_id as usize;
            if idx >= slots.len() {
                return Err(SlotError::NoSuchSlot(slot_id));
            }
            // Gate-only (as in destroy): release() resets the entry below.
            slots[idx].state.transition(SlotState::Empty)?;
            let mut events = Vec::new();
            if slots[idx].live_children > 0 {
                for i in 0..slots.len() {
                    if slots[i].parent == Some(slot_id) && slots[i].state != SlotState::Empty {
                        // Paused/Running -> Faulted are the legal fault edges;
                        // a child already Faulted stays put.
                        if slots[i].state.can_transition(SlotState::Faulted) {
                            slots[i].state = SlotState::Faulted;
                            events.push(Self::info_of(&slots[i], i as u64));
                        }
                        // Orphaned either way: the parent slot id is being
                        // reused; the fault, not the link, carries the truth.
                        slots[i].parent = None;
                    }
                }
            }
            Self::release(&mut slots, idx, &mut events);
            events
        };
        self.publish(events);
        Ok(())
    }

    /// Slot release bookkeeping shared by every destroy/reclaim path:
    /// resets the entry and auto-thaws the parent on last-child release.
    fn release(slots: &mut [SlotEntry], idx: usize, events: &mut Vec<SlotInfo>) {
        let parent = slots[idx].parent;
        slots[idx] = SlotEntry::empty();
        events.push(Self::info_of(&slots[idx], idx as u64));
        if let Some(pid) = parent {
            if let Some(p) = slots.get_mut(pid as usize) {
                p.live_children = p.live_children.saturating_sub(1);
                if p.live_children == 0 && p.state == SlotState::Frozen {
                    // §2.2: freeing the last child unfreezes the parent.
                    p.state = SlotState::Paused;
                    events.push(Self::info_of(p, pid));
                }
            }
        }
    }

    /// The expiry reaper (housekeeping loop): every slot whose lease TTL
    /// elapsed is reclaimed — Paused slots release immediately; Running
    /// slots are marked Faulted (the manager cannot stop a vCPU thread;
    /// the daemon observes the fault and tears down); Faulted slots
    /// release (their lease can never be renewed); Frozen parents are
    /// skipped while children live (each child's own expiry releases it,
    /// and the last release auto-thaws the parent into a reclaimable
    /// Paused). Returns the reclaimed slot ids. A no-op under the v1
    /// no-timeout policy. Returned ids are slots actually released back
    /// to Empty; slots newly marked Faulted are visible via `slot_info`
    /// and are released on the next sweep.
    ///
    /// SINGLE-PASS INVARIANT: both staged handoffs (Running → Faulted →
    /// freed, Frozen → thawed-by-last-child → freed) deliberately
    /// complete on the NEXT sweep — a fixpoint loop here would collapse
    /// the staging and free slots the daemon has not yet observed as
    /// Faulted.
    pub fn reclaim_expired(&self, now_ms: u64) -> Vec<u64> {
        let (reclaimed, events) = {
            let mut slots = self.slots.lock().expect("slot table poisoned");
            let sweep: Vec<(bool, SlotState)> = slots
                .iter()
                .map(|slot| {
                    (
                        matches!(slot.lease, Some((_, Some(at))) if now_ms >= at),
                        slot.state,
                    )
                })
                .collect();
            let mut reclaimed = Vec::new();
            let mut events = Vec::new();
            for idx in 0..slots.len() {
                let (expired, state_at_sweep_start) = sweep[idx];
                if !expired {
                    continue;
                }
                match state_at_sweep_start {
                    SlotState::Paused | SlotState::Faulted if slots[idx].live_children == 0 => {
                        Self::release(&mut slots, idx, &mut events);
                        reclaimed.push(idx as u64);
                    }
                    SlotState::Running => {
                        // The lease stays in place (expired): the NEXT sweep
                        // finds the now-Faulted slot still expired and frees
                        // it — clearing it here would strand the slot where
                        // only force_destroy could reach.
                        slots[idx].state = SlotState::Faulted;
                        events.push(Self::info_of(&slots[idx], idx as u64));
                    }
                    // Frozen with live children: wait — each child's own
                    // expiry releases it, and the last release auto-thaws
                    // this parent into a reclaimable Paused. (Paused with
                    // live_children > 0 cannot exist: fork freezes the
                    // parent atomically under the table lock.)
                    _ => {}
                }
            }
            (reclaimed, events)
        };
        self.publish(events);
        reclaimed
    }

    // ── Introspection (§2.8) ─────────────────────────────────────────────

    pub fn slot_info(&self, slot_id: u64) -> Result<SlotInfo, SlotError> {
        let slots = self.slots.lock().expect("slot table poisoned");
        let entry = slots
            .get(slot_id as usize)
            .ok_or(SlotError::NoSuchSlot(slot_id))?;
        Ok(Self::info_of(entry, slot_id))
    }

    pub fn list(&self) -> Vec<SlotInfo> {
        let slots = self.slots.lock().expect("slot table poisoned");
        slots
            .iter()
            .enumerate()
            .map(|(i, e)| Self::info_of(e, i as u64))
            .collect()
    }

    fn info_of(entry: &SlotEntry, slot_id: u64) -> SlotInfo {
        SlotInfo {
            slot_id,
            state: entry.state,
            icount: entry.icount,
            base_snapshot_id: entry.base_snapshot_id,
            live_children: entry.live_children,
        }
    }
}

/// Default token source: /dev/urandom (host control plane — guest
/// determinism is untouched by host randomness; the token never enters
/// guest-visible state).
fn urandom_token() -> LeaseToken {
    use std::io::Read;
    let mut token = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut token))
        .expect("/dev/urandom must be readable on a Linux worker host");
    token
}

// ── Core pinning (ARCH §9 threads) ──────────────────────────────────────
//
// The pinning SYSCALLS (sched_setaffinity / SCHED_FIFO prio 10) live in
// `dh_vmm::run` — dh-worker forbids unsafe code crate-wide, and thread
// plumbing is run.rs's home (kick signals, tids). THIS module owns the
// slot→core MAP ([`SlotManager::core_for`]); the daemon's vCPU thread
// calls `dh_vmm::run::pin_current_thread(core)` and decides per
// deployment whether an EPERM on the FIFO promotion is fatal
// (CAP_SYS_NICE is present on the lab box, absent on dev machines).

// ── Slot reuse: dirty-tracking reset (restore_engine precondition) ─────

/// Drain and reset a slot's KVM dirty rings so a previously-run slot is
/// safe to restore into — restore_engine's documented precondition is a
/// FRESH slot precisely because stale ring entries would poison the next
/// incremental snapshot. Same-slot reuse is THIS module's job: harvest
/// whatever the ring holds into a throwaway set, then reset the rings
/// VM-wide. Returns the number of stale entries discarded. Call only
/// while the slot is quiescent (Paused, vCPU thread parked). (The
/// restore engine's own host-side RAM writes bypass KVM dirty tracking
/// by design and need no accounting here — this helper's job is solely
/// the stale GUEST-dirtied entries a previous run left in the ring.)
#[cfg(target_arch = "x86_64")]
pub fn reset_slot_dirty_tracking(slot: &dh_vmm::kvm::SlotVm) -> Result<u32, dh_vmm::kvm::KvmError> {
    use dh_vmm::dirty::{harvest_at_boundary, DirtyPageSet, DirtyRing};
    let mut ring = DirtyRing::map(slot)?;
    let mut discard = DirtyPageSet::new(slot.mem_bytes);
    let stats = harvest_at_boundary(&mut ring, &slot.vm, &mut discard)?;
    Ok(stats.harvested)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic token source: counts up so every grant differs.
    fn counting_tokens() -> TokenSource {
        let n = std::sync::atomic::AtomicU64::new(1);
        Box::new(move || {
            let v = n.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let mut t = [0u8; 16];
            t[..8].copy_from_slice(&v.to_le_bytes());
            t
        })
    }

    fn manager(n: usize, policy: LeasePolicy) -> SlotManager {
        SlotManager::with_token_source(n, (2..2 + n as u32).collect(), policy, counting_tokens())
            .unwrap()
    }

    #[test]
    fn defaults_match_arch_section_9() {
        // Lab box: 6 physical cores → 4 slots on cores 2-5.
        assert_eq!(default_slot_count(6), 4);
        assert_eq!(default_slot_count(2), 1); // floor, never 0
        assert_eq!(parse_core_list("2-5"), Some(vec![2, 3, 4, 5]));
        assert_eq!(parse_core_list("2,4-5"), Some(vec![2, 4, 5]));
        assert_eq!(parse_core_list(""), None);
        assert_eq!(parse_core_list("5-2"), None);
        assert_eq!(parse_core_list("2,2"), None);
        assert_eq!(parse_core_list("2-3,3"), None);
        // Dedicated core per slot, no oversubscription.
        assert!(matches!(
            SlotManager::new(5, vec![2, 3, 4, 5], LeasePolicy::default()),
            Err(SlotError::NotEnoughCores { slots: 5, cores: 4 })
        ));
        assert!(matches!(
            SlotManager::new(2, vec![2, 2], LeasePolicy::default()),
            Err(SlotError::DuplicateCore { core: 2 })
        ));
    }

    #[test]
    fn allocate_grants_distinct_leases_until_exhaustion() {
        let m = manager(2, LeasePolicy::default());
        let a = m.allocate(0).unwrap();
        let b = m.allocate(0).unwrap();
        assert_ne!(a.slot_id, b.slot_id);
        assert_ne!(a.token, b.token);
        assert_eq!(m.allocate(0), Err(SlotError::NoFreeSlot));
        assert_eq!(m.slot_info(a.slot_id).unwrap().state, SlotState::Paused);
        // Destroy returns the slot to the pool.
        m.check_destroy(&a, 0).unwrap();
        assert_eq!(m.slot_info(a.slot_id).unwrap().state, SlotState::Paused);
        m.destroy(&a, 0).unwrap();
        assert_eq!(m.slot_info(a.slot_id).unwrap().state, SlotState::Empty);
        assert!(m.allocate(0).is_ok());
    }

    #[test]
    fn stale_token_is_failed_precondition_everywhere() {
        let m = manager(1, LeasePolicy::default());
        let lease = m.allocate(0).unwrap();
        let stale = Lease {
            slot_id: lease.slot_id,
            token: [0xFF; 16],
        };
        let err = SlotError::StaleLease { slot_id: 0 };
        assert_eq!(m.validate(&stale, 0), Err(err));
        assert!(matches!(
            m.mark_running(&stale, 0),
            Err(SlotError::StaleLease { .. })
        ));
        assert!(matches!(
            m.destroy(&stale, 0),
            Err(SlotError::StaleLease { .. })
        ));
        assert!(matches!(
            m.fork(&stale, 1, 0),
            Err(SlotError::StaleLease { .. })
        ));
        assert!(matches!(
            m.check_fork(&stale, 1, 0),
            Err(SlotError::StaleLease { .. })
        ));
        assert!(matches!(
            m.fork(&stale, 0, 0),
            Err(SlotError::StaleLease { .. })
        ));
        // A second allocate cannot steal slot 0 (it is not Empty), and
        // the REAL token still works after every stale attempt.
        assert!(m.validate(&lease, 0).is_ok());
    }

    #[test]
    fn run_pause_crossings_follow_the_state_machine() {
        let m = manager(1, LeasePolicy::default());
        let lease = m.allocate(0).unwrap();
        m.mark_running(&lease, 0).unwrap();
        assert_eq!(m.slot_info(0).unwrap().state, SlotState::Running);
        // Running → Running is a state-machine bug, loudly.
        assert!(matches!(
            m.mark_running(&lease, 0),
            Err(SlotError::State(SlotStateError::InvalidTransition { .. }))
        ));
        m.mark_paused(&lease, 0).unwrap();
        // Destroy of a Running slot is refused (pause first).
        m.mark_running(&lease, 0).unwrap();
        assert!(matches!(m.destroy(&lease, 0), Err(SlotError::State(_))));
    }

    #[test]
    fn fork_freezes_parent_accounts_children_and_autothaws() {
        let m = manager(4, LeasePolicy::default());
        let parent = m.allocate(0).unwrap();
        m.check_fork(&parent, 2, 0).unwrap();
        assert_eq!(
            m.slot_info(parent.slot_id).unwrap().state,
            SlotState::Paused
        );
        assert_eq!(m.slot_info(parent.slot_id).unwrap().live_children, 0);
        let children = m.fork(&parent, 2, 0).unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(
            m.slot_info(parent.slot_id).unwrap().state,
            SlotState::Frozen
        );
        assert_eq!(m.slot_info(parent.slot_id).unwrap().live_children, 2);
        for c in &children {
            assert_eq!(m.slot_info(c.slot_id).unwrap().state, SlotState::Paused);
        }

        // R9: the frozen parent refuses writes and ordinary destroy.
        assert!(matches!(
            m.checkout_write(&parent, "Run", 0),
            Err(SlotError::State(SlotStateError::FrozenWriteDenied { .. }))
        ));
        assert_eq!(
            m.destroy(&parent, 0),
            Err(SlotError::LiveChildren {
                slot_id: parent.slot_id,
                children: 2
            })
        );

        // A CoW child cannot be a fork parent — and the refusal names
        // the REAL reason (the fork_engine CoW invariant), not a
        // fabricated state-machine error.
        assert_eq!(
            m.fork(&children[0], 1, 0),
            Err(SlotError::CowChildCannotFork {
                slot_id: children[0].slot_id
            })
        );

        // Destroying the last child auto-thaws the parent.
        m.destroy(&children[0], 0).unwrap();
        assert_eq!(
            m.slot_info(parent.slot_id).unwrap().state,
            SlotState::Frozen
        );
        m.destroy(&children[1], 0).unwrap();
        assert_eq!(
            m.slot_info(parent.slot_id).unwrap().state,
            SlotState::Paused
        );
        assert_eq!(m.slot_info(parent.slot_id).unwrap().live_children, 0);
        m.destroy(&parent, 0).unwrap();
    }

    #[test]
    fn fork_is_all_or_nothing_when_slots_run_out() {
        let m = manager(2, LeasePolicy::default());
        let parent = m.allocate(0).unwrap();
        // Only 1 Empty slot remains; asking for 2 children must change
        // NOTHING (parent stays Paused, no orphan child leases).
        assert_eq!(m.fork(&parent, 2, 0), Err(SlotError::NoFreeSlot));
        assert_eq!(
            m.slot_info(parent.slot_id).unwrap().state,
            SlotState::Paused
        );
        assert_eq!(m.slot_info(parent.slot_id).unwrap().live_children, 0);
        assert_eq!(
            m.list()
                .iter()
                .filter(|i| i.state == SlotState::Empty)
                .count(),
            1
        );
        // And a 1-child fork still works afterward.
        assert_eq!(m.fork(&parent, 1, 0).unwrap().len(), 1);
    }

    #[test]
    fn zero_child_fork_is_refused_before_any_mutation() {
        let m = manager(2, LeasePolicy::default());
        let parent = m.allocate(0).unwrap();
        let missing = Lease {
            slot_id: 99,
            token: parent.token,
        };
        assert_eq!(m.fork(&missing, 0, 0), Err(SlotError::NoSuchSlot(99)));
        // A zero-child fork would freeze the parent with nothing to
        // ever thaw it — refused outright, parent untouched.
        assert_eq!(
            m.fork(&parent, 0, 0),
            Err(SlotError::ZeroChildFork {
                slot_id: parent.slot_id
            })
        );
        assert_eq!(
            m.slot_info(parent.slot_id).unwrap().state,
            SlotState::Paused
        );
        assert_eq!(m.slot_info(parent.slot_id).unwrap().live_children, 0);
        // The parent still runs and forks normally afterward.
        m.mark_running(&parent, 0).unwrap();
        m.mark_paused(&parent, 0).unwrap();
        assert_eq!(m.fork(&parent, 1, 0).unwrap().len(), 1);
    }

    /// Locks the cross-tenant invariant: force_destroying a frozen
    /// parent orphans its children (parent = None), so destroying an
    /// OLD orphan later can never decrement — or auto-thaw — a NEW
    /// tenant that reuses the parent's slot id.
    #[test]
    fn orphaned_child_destroy_cannot_touch_a_reused_parent_slot() {
        let m = manager(4, LeasePolicy::default());
        let parent = m.allocate(0).unwrap();
        let orphans = m.fork(&parent, 1, 0).unwrap();
        m.force_destroy(parent.slot_id).unwrap();
        assert_eq!(
            m.slot_info(orphans[0].slot_id).unwrap().state,
            SlotState::Faulted
        );

        // A NEW tenant claims the freed slot id and becomes a frozen
        // parent itself.
        let tenant = m.allocate(0).unwrap();
        assert_eq!(tenant.slot_id, parent.slot_id, "slot id reused");
        let tenant_children = m.fork(&tenant, 1, 0).unwrap();
        assert_eq!(m.slot_info(tenant.slot_id).unwrap().live_children, 1);

        // Destroying the OLD orphan must not touch the new tenant.
        m.destroy(&orphans[0], 0).unwrap();
        assert_eq!(
            m.slot_info(tenant.slot_id).unwrap().state,
            SlotState::Frozen
        );
        assert_eq!(m.slot_info(tenant.slot_id).unwrap().live_children, 1);

        // And the real child still thaws it.
        m.destroy(&tenant_children[0], 0).unwrap();
        assert_eq!(
            m.slot_info(tenant.slot_id).unwrap().state,
            SlotState::Paused
        );
    }

    #[test]
    fn faults_are_not_lease_gated_and_only_destroy_exits() {
        let m = manager(1, LeasePolicy::default());
        let lease = m.allocate(0).unwrap();
        m.mark_running(&lease, 0).unwrap();
        m.mark_faulted(0).unwrap();
        assert_eq!(m.slot_info(0).unwrap().state, SlotState::Faulted);
        // No resurrection.
        assert!(m.mark_running(&lease, 0).is_err());
        assert!(m.mark_paused(&lease, 0).is_err());
        assert!(matches!(
            m.checkout_write(&lease, "Run", 0),
            Err(SlotError::State(SlotStateError::FaultedWriteDenied { .. }))
        ));
        // Destroy (with the still-valid lease) is the only exit.
        m.destroy(&lease, 0).unwrap();
        assert_eq!(m.slot_info(0).unwrap().state, SlotState::Empty);
    }

    #[test]
    fn force_destroy_cascades_faults_to_cow_children() {
        let m = manager(3, LeasePolicy::default());
        let parent = m.allocate(0).unwrap();
        let children = m.fork(&parent, 2, 0).unwrap();
        // Host-integrity loss on the frozen parent: children's baseline
        // is gone — they fault, the parent slot frees.
        m.force_destroy(parent.slot_id).unwrap();
        assert_eq!(m.slot_info(parent.slot_id).unwrap().state, SlotState::Empty);
        for c in &children {
            assert_eq!(m.slot_info(c.slot_id).unwrap().state, SlotState::Faulted);
        }
        // The faulted orphans still destroy normally with their leases.
        for c in &children {
            m.destroy(c, 0).unwrap();
        }
    }

    #[test]
    fn v1_policy_never_expires() {
        let m = manager(1, LeasePolicy::default());
        let lease = m.allocate(0).unwrap();
        assert!(m.validate(&lease, u64::MAX).is_ok());
        assert_eq!(m.reclaim_expired(u64::MAX), Vec::<u64>::new());
        // renew is a harmless no-op.
        m.renew(&lease, u64::MAX).unwrap();
        assert!(m.validate(&lease, u64::MAX).is_ok());
    }

    #[test]
    fn ttl_expiry_renew_and_reclaim() {
        let m = manager(2, LeasePolicy::with_ttl(100));
        let a = m.allocate(0).unwrap();
        let b = m.allocate(0).unwrap();

        // Renewal slides a's expiry; b lapses at 100.
        m.renew(&a, 90).unwrap(); // a now expires at 190
        assert_eq!(
            m.validate(&b, 100),
            Err(SlotError::LeaseExpired { slot_id: b.slot_id })
        );
        assert!(m.validate(&a, 100).is_ok());

        // An expired lease cannot be renewed back to life.
        assert_eq!(
            m.renew(&b, 150),
            Err(SlotError::LeaseExpired { slot_id: b.slot_id })
        );

        // Reclaim at 100: only b (Paused) frees; a survives until 190.
        assert_eq!(m.reclaim_expired(100), vec![b.slot_id]);
        assert_eq!(m.slot_info(b.slot_id).unwrap().state, SlotState::Empty);
        assert!(m.validate(&a, 150).is_ok());
        assert_eq!(m.reclaim_expired(190), vec![a.slot_id]);

        let overflow = manager(1, LeasePolicy::with_ttl(100));
        let lease = overflow.allocate(u64::MAX - 1).unwrap();
        assert!(overflow.validate(&lease, u64::MAX - 1).is_ok());
        assert_eq!(
            overflow.validate(&lease, u64::MAX),
            Err(SlotError::LeaseExpired {
                slot_id: lease.slot_id
            })
        );
    }

    #[test]
    fn reclaim_faults_running_slots_and_waits_for_frozen_parents() {
        let m = manager(3, LeasePolicy::with_ttl(100));
        let runner = m.allocate(0).unwrap();
        m.mark_running(&runner, 0).unwrap();
        let parent = m.allocate(0).unwrap();
        let children = m.fork(&parent, 1, 0).unwrap();

        // At 100 everything is expired. The runner faults (the manager
        // cannot stop a vCPU thread) but is not reclaimed yet; the
        // frozen parent waits; the child (Paused) frees — which
        // auto-thaws the parent.
        let mut got = m.reclaim_expired(100);
        got.sort_unstable();
        let mut want = vec![children[0].slot_id];
        want.sort_unstable();
        assert_eq!(got, want);
        assert_eq!(
            m.slot_info(runner.slot_id).unwrap().state,
            SlotState::Faulted
        );
        assert_eq!(
            m.slot_info(parent.slot_id).unwrap().state,
            SlotState::Paused
        );

        // Next sweep: the faulted runner and the thawed parent free.
        let mut got = m.reclaim_expired(101);
        got.sort_unstable();
        let mut want = vec![runner.slot_id, parent.slot_id];
        want.sort_unstable();
        assert_eq!(got, want);
        assert!(m.list().iter().all(|i| i.state == SlotState::Empty));
    }

    #[test]
    fn reclaim_stages_parent_release_even_when_child_slot_is_lower() {
        let m = manager(3, LeasePolicy::with_ttl(100));
        let filler = m.allocate(0).unwrap();
        let parent = m.allocate(0).unwrap();
        m.destroy(&filler, 0).unwrap();
        let children = m.fork(&parent, 1, 0).unwrap();
        assert!(
            children[0].slot_id < parent.slot_id,
            "test needs child before parent in sweep order"
        );

        assert_eq!(m.reclaim_expired(100), vec![children[0].slot_id]);
        assert_eq!(
            m.slot_info(parent.slot_id).unwrap().state,
            SlotState::Paused
        );

        assert_eq!(m.reclaim_expired(101), vec![parent.slot_id]);
        assert!(m.list().iter().all(|i| i.state == SlotState::Empty));
    }

    #[test]
    fn position_flows_into_slot_info() {
        let m = manager(2, LeasePolicy::default());
        let lease = m.allocate(0).unwrap();
        m.set_position(&lease, 42_000, Some([0xAB; 32]), 0).unwrap();
        let info = m.slot_info(lease.slot_id).unwrap();
        assert_eq!(info.icount, 42_000);
        assert_eq!(info.base_snapshot_id, Some([0xAB; 32]));
        // Children inherit the parent's position at fork.
        let children = m.fork(&lease, 1, 0).unwrap();
        let child_info = m.slot_info(children[0].slot_id).unwrap();
        assert_eq!(child_info.icount, 42_000);
        assert_eq!(child_info.base_snapshot_id, Some([0xAB; 32]));
        // And the released slot reports a clean row.
        m.destroy(&children[0], 0).unwrap();
        assert_eq!(
            m.slot_info(children[0].slot_id).unwrap(),
            SlotInfo {
                slot_id: children[0].slot_id,
                state: SlotState::Empty,
                icount: 0,
                base_snapshot_id: None,
                live_children: 0,
            }
        );
    }

    #[test]
    fn core_map_is_slot_ordered() {
        let m = manager(3, LeasePolicy::default());
        assert_eq!(m.core_for(0), Some(2));
        assert_eq!(m.core_for(2), Some(4));
        assert_eq!(m.core_for(3), None);
    }
}
