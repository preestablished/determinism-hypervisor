//! Daemon-owned per-slot runtime table (bead 8kb).
//!
//! `SlotManager` remains the single state-machine and lease authority. This
//! table is the daemon's resource owner for the matching slot ids: KVM fds,
//! devices, entropy stream, dirty tracking, counters, and current lineage
//! position. Mutating RPCs should check the lease/state through
//! `SlotManager`, then enter this table from a blocking worker thread before
//! driving KVM or snapshot-store work.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeError {
    NoSuchSlot(u64),
    Empty { slot_id: u64 },
    Occupied { slot_id: u64 },
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::NoSuchSlot(slot_id) => write!(f, "runtime slot {slot_id} does not exist"),
            RuntimeError::Empty { slot_id } => write!(f, "runtime slot {slot_id} is empty"),
            RuntimeError::Occupied { slot_id } => write!(f, "runtime slot {slot_id} is occupied"),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// Fixed-size table keyed by `SlotManager` slot id.
pub struct RuntimeTable<T> {
    slots: Mutex<Vec<Option<T>>>,
}

impl<T> RuntimeTable<T> {
    pub fn new(slot_count: usize) -> Self {
        Self {
            slots: Mutex::new((0..slot_count).map(|_| None).collect()),
        }
    }

    pub fn slot_count(&self) -> usize {
        self.slots.lock().expect("runtime table poisoned").len()
    }

    pub fn occupied_count(&self) -> usize {
        self.slots
            .lock()
            .expect("runtime table poisoned")
            .iter()
            .filter(|slot| slot.is_some())
            .count()
    }

    pub fn ensure_occupied(&self, slot_id: u64) -> Result<(), RuntimeError> {
        let slots = self.slots.lock().expect("runtime table poisoned");
        let entry = slots
            .get(slot_id as usize)
            .ok_or(RuntimeError::NoSuchSlot(slot_id))?;
        entry
            .as_ref()
            .map(|_| ())
            .ok_or(RuntimeError::Empty { slot_id })
    }

    pub fn insert(&self, slot_id: u64, runtime: T) -> Result<(), RuntimeError> {
        let mut slots = self.slots.lock().expect("runtime table poisoned");
        let entry = slots
            .get_mut(slot_id as usize)
            .ok_or(RuntimeError::NoSuchSlot(slot_id))?;
        if entry.is_some() {
            return Err(RuntimeError::Occupied { slot_id });
        }
        *entry = Some(runtime);
        Ok(())
    }

    pub fn insert_many(&self, runtimes: Vec<(u64, T)>) -> Result<(), RuntimeError> {
        let mut slots = self.slots.lock().expect("runtime table poisoned");
        let mut seen = Vec::with_capacity(runtimes.len());
        for (slot_id, _) in &runtimes {
            let entry = slots
                .get(*slot_id as usize)
                .ok_or(RuntimeError::NoSuchSlot(*slot_id))?;
            if entry.is_some() || seen.contains(slot_id) {
                return Err(RuntimeError::Occupied { slot_id: *slot_id });
            }
            seen.push(*slot_id);
        }
        for (slot_id, runtime) in runtimes {
            slots[slot_id as usize] = Some(runtime);
        }
        Ok(())
    }

    pub fn take(&self, slot_id: u64) -> Result<T, RuntimeError> {
        let mut slots = self.slots.lock().expect("runtime table poisoned");
        let entry = slots
            .get_mut(slot_id as usize)
            .ok_or(RuntimeError::NoSuchSlot(slot_id))?;
        entry.take().ok_or(RuntimeError::Empty { slot_id })
    }

    pub fn with<R>(&self, slot_id: u64, f: impl FnOnce(&T) -> R) -> Result<R, RuntimeError> {
        let slots = self.slots.lock().expect("runtime table poisoned");
        let runtime = slots
            .get(slot_id as usize)
            .ok_or(RuntimeError::NoSuchSlot(slot_id))?
            .as_ref()
            .ok_or(RuntimeError::Empty { slot_id })?;
        Ok(f(runtime))
    }

    pub fn with_mut<R>(
        &self,
        slot_id: u64,
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, RuntimeError> {
        let mut slots = self.slots.lock().expect("runtime table poisoned");
        let runtime = slots
            .get_mut(slot_id as usize)
            .ok_or(RuntimeError::NoSuchSlot(slot_id))?
            .as_mut()
            .ok_or(RuntimeError::Empty { slot_id })?;
        Ok(f(runtime))
    }
}

pub type WorkerRuntimeTable = RuntimeTable<SlotRuntime>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotPosition {
    /// Cumulative lineage icount at the current deterministic boundary.
    pub cumulative_icount: u64,
    /// Segment-relative icount, reset to zero by restore/fork.
    pub segment_icount: u64,
    pub vns: u64,
    pub epoch_index: u64,
    pub frame_counter: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeThreadState {
    Parked,
    Running { tid: i32 },
    PauseRequested { tid: i32 },
    Faulted(String),
}

impl Default for RuntimeThreadState {
    fn default() -> Self {
        Self::Parked
    }
}

/// The real resources owned by one worker slot.
pub struct SlotRuntime {
    pub slot: dh_vmm::kvm::SlotVm,
    pub bus: dh_devices::MmioBus,
    pub entropy: dh_devices::entropy::DetEntropy,
    pub machine_config: dh_vmm::config::MachineConfig,
    pub dirty_ring: dh_vmm::dirty::DirtyRing,
    pub dirty: dh_vmm::dirty::DirtyPageSet,
    pub chain: dh_vmm::hash::StateHashChain,
    pub counter: Option<dh_detclock::counter::InstRetired>,
    pub base_snapshot: Option<snapstore_types::SnapshotRef>,
    pub position: SlotPosition,
    pub thread: RuntimeThreadState,
    pause: Arc<AtomicBool>,
}

impl SlotRuntime {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        slot: dh_vmm::kvm::SlotVm,
        bus: dh_devices::MmioBus,
        entropy: dh_devices::entropy::DetEntropy,
        machine_config: dh_vmm::config::MachineConfig,
        chain: dh_vmm::hash::StateHashChain,
        counter: Option<dh_detclock::counter::InstRetired>,
        base_snapshot: Option<snapstore_types::SnapshotRef>,
        position: SlotPosition,
    ) -> Result<Self, dh_vmm::kvm::KvmError> {
        let dirty_ring = dh_vmm::dirty::DirtyRing::map(&slot)?;
        let dirty = dh_vmm::dirty::DirtyPageSet::new(slot.mem_bytes);
        Ok(Self::from_parts(SlotRuntimeParts {
            slot,
            bus,
            entropy,
            machine_config,
            dirty_ring,
            dirty,
            chain,
            counter,
            base_snapshot,
            position,
            thread: RuntimeThreadState::Parked,
            pause: Arc::new(AtomicBool::new(false)),
        }))
    }

    pub fn from_parts(parts: SlotRuntimeParts) -> Self {
        Self {
            slot: parts.slot,
            bus: parts.bus,
            entropy: parts.entropy,
            machine_config: parts.machine_config,
            dirty_ring: parts.dirty_ring,
            dirty: parts.dirty,
            chain: parts.chain,
            counter: parts.counter,
            base_snapshot: parts.base_snapshot,
            position: parts.position,
            thread: parts.thread,
            pause: parts.pause,
        }
    }

    pub fn pause_flag(&self) -> Arc<AtomicBool> {
        self.pause.clone()
    }

    pub fn request_pause(&mut self) {
        self.pause.store(true, Ordering::SeqCst);
        if let RuntimeThreadState::Running { tid } = self.thread {
            self.thread = RuntimeThreadState::PauseRequested { tid };
        }
    }

    pub fn clear_pause_request(&mut self) {
        self.pause.store(false, Ordering::SeqCst);
        if let RuntimeThreadState::PauseRequested { tid } = self.thread {
            self.thread = RuntimeThreadState::Running { tid };
        }
    }

    pub fn boundary_state(&self, agenda_empty: bool) -> crate::snapshot_engine::BoundaryState {
        crate::snapshot_engine::BoundaryState {
            icount: self.position.cumulative_icount,
            vns: self.position.vns,
            epoch_index: self.position.epoch_index,
            hash_chain: self.chain.value(),
            agenda_empty,
        }
    }

    pub fn set_boundary(
        &mut self,
        cumulative_icount: u64,
        segment_icount: u64,
        vns: u64,
        epoch_index: u64,
        chain: dh_vmm::hash::StateHashChain,
    ) {
        self.position.cumulative_icount = cumulative_icount;
        self.position.segment_icount = segment_icount;
        self.position.vns = vns;
        self.position.epoch_index = epoch_index;
        self.chain = chain;
    }

    pub fn state_hash(&self) -> [u8; 32] {
        self.chain.value()
    }
}

pub struct SlotRuntimeParts {
    pub slot: dh_vmm::kvm::SlotVm,
    pub bus: dh_devices::MmioBus,
    pub entropy: dh_devices::entropy::DetEntropy,
    pub machine_config: dh_vmm::config::MachineConfig,
    pub dirty_ring: dh_vmm::dirty::DirtyRing,
    pub dirty: dh_vmm::dirty::DirtyPageSet,
    pub chain: dh_vmm::hash::StateHashChain,
    pub counter: Option<dh_detclock::counter::InstRetired>,
    pub base_snapshot: Option<snapstore_types::SnapshotRef>,
    pub position: SlotPosition,
    pub thread: RuntimeThreadState,
    pub pause: Arc<AtomicBool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct Stub {
        value: u32,
    }

    #[test]
    fn table_enforces_fixed_slot_ownership() {
        let table = RuntimeTable::new(2);
        assert_eq!(table.slot_count(), 2);
        assert_eq!(table.occupied_count(), 0);
        assert_eq!(
            table.ensure_occupied(0),
            Err(RuntimeError::Empty { slot_id: 0 })
        );
        assert_eq!(
            table.insert(9, Stub { value: 1 }),
            Err(RuntimeError::NoSuchSlot(9))
        );

        table.insert(0, Stub { value: 7 }).unwrap();
        assert_eq!(table.occupied_count(), 1);
        assert_eq!(
            table.insert(0, Stub { value: 8 }),
            Err(RuntimeError::Occupied { slot_id: 0 })
        );
        assert_eq!(table.with(0, |s| s.value).unwrap(), 7);
        table.with_mut(0, |s| s.value = 11).unwrap();
        assert_eq!(table.take(0).unwrap().value, 11);
        assert_eq!(table.occupied_count(), 0);
        assert_eq!(table.take(0), Err(RuntimeError::Empty { slot_id: 0 }));
    }

    #[test]
    fn insert_many_is_all_or_nothing() {
        let table = RuntimeTable::new(3);
        table.insert(1, Stub { value: 10 }).unwrap();
        assert_eq!(
            table.insert_many(vec![(0, Stub { value: 20 }), (1, Stub { value: 30 })]),
            Err(RuntimeError::Occupied { slot_id: 1 })
        );
        assert_eq!(table.occupied_count(), 1);
        assert_eq!(
            table.ensure_occupied(0),
            Err(RuntimeError::Empty { slot_id: 0 })
        );

        table
            .insert_many(vec![(0, Stub { value: 20 }), (2, Stub { value: 30 })])
            .unwrap();
        assert_eq!(table.occupied_count(), 3);
        assert_eq!(table.with(0, |s| s.value).unwrap(), 20);
        assert_eq!(table.with(2, |s| s.value).unwrap(), 30);
    }

    #[test]
    fn insert_many_rejects_duplicate_slot_ids() {
        let table = RuntimeTable::new(2);
        assert_eq!(
            table.insert_many(vec![(0, Stub { value: 1 }), (0, Stub { value: 2 })]),
            Err(RuntimeError::Occupied { slot_id: 0 })
        );
        assert_eq!(table.occupied_count(), 0);
    }
}
