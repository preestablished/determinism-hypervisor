//! Daemon-owned per-slot runtime table (bead 8kb).
//!
//! `SlotManager` remains the single state-machine and lease authority. This
//! table is the daemon's resource owner for the matching slot ids: KVM fds,
//! devices, entropy stream, dirty tracking, counters, and current lineage
//! position. Mutating RPCs should check the lease/state through
//! `SlotManager`, then enter this table from a blocking worker thread before
//! driving KVM or snapshot-store work.

use std::any::Any;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};

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
        let mut seen = HashSet::with_capacity(runtimes.len());
        for (slot_id, _) in &runtimes {
            let entry = slots
                .get(*slot_id as usize)
                .ok_or(RuntimeError::NoSuchSlot(*slot_id))?;
            if entry.is_some() || !seen.insert(*slot_id) {
                return Err(RuntimeError::Occupied { slot_id: *slot_id });
            }
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

pub type WorkerRuntimeTable = RuntimeTable<Arc<SlotActor>>;

#[derive(Debug)]
pub enum RuntimeActorError {
    Start(String),
    Send(String),
    Recv(String),
    TypeMismatch,
    Join(String),
}

impl std::fmt::Display for RuntimeActorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeActorError::Start(e) => write!(f, "runtime actor start failed: {e}"),
            RuntimeActorError::Send(e) => write!(f, "runtime actor send failed: {e}"),
            RuntimeActorError::Recv(e) => write!(f, "runtime actor receive failed: {e}"),
            RuntimeActorError::TypeMismatch => write!(f, "runtime actor reply type mismatch"),
            RuntimeActorError::Join(e) => write!(f, "runtime actor join failed: {e}"),
        }
    }
}

impl std::error::Error for RuntimeActorError {}

type ActorReply = mpsc::Sender<Box<dyn Any + Send>>;
type RuntimeOp = Box<dyn FnOnce(&mut SlotRuntime) -> Box<dyn Any + Send> + Send + 'static>;

enum ActorCommand {
    WithRuntime { op: RuntimeOp, reply: ActorReply },
    Shutdown { reply: mpsc::Sender<SlotRuntime> },
}

/// Stable per-slot execution owner.
///
/// The actor thread owns the `SlotRuntime`, the vCPU fd, and the
/// thread-attached `InstRetired` counter. RPC handlers communicate by
/// sending closures to this thread; they never run guest work on Tokio's
/// blocking pool.
pub struct SlotActor {
    slot_id: u64,
    tid: i32,
    pause: Arc<AtomicBool>,
    tx: mpsc::Sender<ActorCommand>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl SlotActor {
    pub fn start(slot_id: u64, core: u32, runtime: SlotRuntime) -> Result<Self, RuntimeActorError> {
        let pause = runtime.pause_flag();
        let (tx, rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let builder = thread::Builder::new().name(format!("dh-slot-{slot_id}"));
        let join = builder
            .spawn(move || actor_main(slot_id, core, runtime, rx, started_tx))
            .map_err(|e| RuntimeActorError::Start(e.to_string()))?;
        let tid = match started_rx.recv() {
            Ok(Ok(tid)) => tid,
            Ok(Err(e)) => {
                let _ = join.join();
                return Err(RuntimeActorError::Start(e));
            }
            Err(e) => {
                let _ = join.join();
                return Err(RuntimeActorError::Recv(e.to_string()));
            }
        };
        Ok(Self {
            slot_id,
            tid,
            pause,
            tx,
            join: Mutex::new(Some(join)),
        })
    }

    pub fn slot_id(&self) -> u64 {
        self.slot_id
    }

    pub fn tid(&self) -> i32 {
        self.tid
    }

    pub fn request_pause(&self) {
        self.pause.store(true, Ordering::SeqCst);
    }

    pub fn with_runtime<R>(
        &self,
        f: impl FnOnce(&SlotRuntime) -> R + Send + 'static,
    ) -> Result<R, RuntimeActorError>
    where
        R: Send + 'static,
    {
        self.with_runtime_mut(move |runtime| f(runtime))
    }

    pub fn with_runtime_mut<R>(
        &self,
        f: impl FnOnce(&mut SlotRuntime) -> R + Send + 'static,
    ) -> Result<R, RuntimeActorError>
    where
        R: Send + 'static,
    {
        let (reply, rx) = mpsc::channel();
        let op: RuntimeOp = Box::new(move |runtime| Box::new(f(runtime)));
        self.tx
            .send(ActorCommand::WithRuntime { op, reply })
            .map_err(|e| RuntimeActorError::Send(e.to_string()))?;
        let boxed = rx
            .recv()
            .map_err(|e| RuntimeActorError::Recv(e.to_string()))?;
        boxed
            .downcast::<R>()
            .map(|boxed| *boxed)
            .map_err(|_| RuntimeActorError::TypeMismatch)
    }

    pub fn shutdown(self) -> Result<SlotRuntime, RuntimeActorError> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(ActorCommand::Shutdown { reply })
            .map_err(|e| RuntimeActorError::Send(e.to_string()))?;
        let runtime = rx
            .recv()
            .map_err(|e| RuntimeActorError::Recv(e.to_string()))?;
        if let Some(join) = self
            .join
            .lock()
            .expect("runtime actor join poisoned")
            .take()
        {
            join.join().map_err(|e| {
                RuntimeActorError::Join(
                    e.downcast_ref::<&str>()
                        .copied()
                        .or_else(|| e.downcast_ref::<String>().map(String::as_str))
                        .unwrap_or("panic")
                        .to_string(),
                )
            })?;
        }
        Ok(runtime)
    }
}

impl Drop for SlotActor {
    fn drop(&mut self) {
        let Some(join) = self
            .join
            .lock()
            .expect("runtime actor join poisoned")
            .take()
        else {
            return;
        };
        let (reply, _rx) = mpsc::channel();
        let _ = self.tx.send(ActorCommand::Shutdown { reply });
        let _ = join.join();
    }
}

fn actor_main(
    slot_id: u64,
    core: u32,
    mut runtime: SlotRuntime,
    rx: mpsc::Receiver<ActorCommand>,
    started: mpsc::Sender<Result<i32, String>>,
) {
    let tid = dh_vmm::run::current_tid();
    let start = (|| {
        dh_vmm::run::install_kick_handler().map_err(|e| format!("install kick handler: {e}"))?;
        dh_vmm::run::pin_current_thread(core)
            .map_err(|e| format!("pin slot {slot_id} actor to core {core}: {e:?}"))?;
        let _ = dh_vmm::run::set_current_thread_fifo();
        let counter = dh_detclock::counter::InstRetired::open_for_current_thread()
            .map_err(|e| format!("open InstRetired on slot {slot_id} actor: {e:?}"))?;
        counter
            .route_overflow_to_thread(tid, dh_vmm::run::kick_signal())
            .map_err(|e| format!("route InstRetired overflow to tid {tid}: {e:?}"))?;
        counter
            .reset()
            .map_err(|e| format!("reset InstRetired on slot {slot_id}: {e:?}"))?;
        counter
            .arm_period(dh_detclock::counter::NEVER_FIRES_PERIOD)
            .map_err(|e| format!("arm InstRetired on slot {slot_id}: {e:?}"))?;
        counter
            .enable()
            .map_err(|e| format!("enable InstRetired on slot {slot_id}: {e:?}"))?;
        runtime.counter = Some(counter);
        runtime.thread = RuntimeThreadState::Parked;
        runtime.pause.store(false, Ordering::SeqCst);
        Ok(tid)
    })();
    if started.send(start).is_err() {
        return;
    }

    while let Ok(cmd) = rx.recv() {
        match cmd {
            ActorCommand::WithRuntime { op, reply } => {
                let _ = reply.send(op(&mut runtime));
            }
            ActorCommand::Shutdown { reply } => {
                let _ = reply.send(runtime);
                break;
            }
        }
    }
}

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrainedGuestEvent {
    pub stream: u32,
    pub icount: u64,
    pub vns: u64,
    pub payload: Vec<u8>,
}

#[cfg(target_arch = "x86_64")]
pub fn runtime_hash_device_sections(
    bus: &dh_devices::MmioBus,
    lapic: &dh_vmm::lapic::LocalApic,
) -> Vec<u8> {
    let mut bytes = dh_vmm::hash::lapic_section(lapic);
    bytes.extend_from_slice(&dh_vmm::hash::device_sections(bus));
    bytes
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueuedInputAt {
    Icount(u64),
    Frame(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueuedInputKind {
    PadSet {
        port: u8,
        buttons: u32,
        frame_hint: u32,
    },
    NetRx {
        frame: Vec<u8>,
    },
    DevEvent {
        device_id: u16,
        event_type: u16,
        payload: Vec<u8>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedInput {
    pub at: QueuedInputAt,
    pub order: u64,
    pub kind: QueuedInputKind,
}

/// The real resources owned by one worker slot.
pub struct SlotRuntime {
    pub slot: dh_vmm::kvm::SlotVm,
    pub bus: dh_devices::MmioBus,
    pub lapic: dh_vmm::lapic::LocalApic,
    pub entropy: dh_devices::entropy::DetEntropy,
    /// Active DHILOG segment for this slot. Lifecycle RPCs create a fresh
    /// segment; Run wiring records into it and TakeSnapshot seals it.
    pub log: Option<dh_inputlog::dhilog::LogWriter>,
    pub machine_config: dh_vmm::config::MachineConfig,
    pub dirty_ring: dh_vmm::dirty::DirtyRing,
    pub dirty: dh_vmm::dirty::DirtyPageSet,
    pub chain: dh_vmm::hash::StateHashChain,
    pub counter: Option<dh_detclock::counter::InstRetired>,
    pub base_snapshot: Option<snapstore_types::SnapshotRef>,
    pub position: SlotPosition,
    pub bisection_checkpoint_anchor_icount: u64,
    pub queued_inputs: Vec<QueuedInput>,
    /// Drained detchannel events not yet selected by StreamGuestEvents.
    /// The worker keeps only the newest capped backlog per slot; matching
    /// events are consumed when the RPC response is constructed.
    pub guest_events: Vec<DrainedGuestEvent>,
    pub next_input_order: u64,
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
            lapic: dh_vmm::lapic::LocalApic::new(),
            entropy,
            log: None,
            machine_config,
            dirty_ring,
            dirty,
            chain,
            counter,
            base_snapshot,
            position,
            bisection_checkpoint_anchor_icount: 0,
            queued_inputs: Vec::new(),
            guest_events: Vec::new(),
            next_input_order: 0,
            thread: RuntimeThreadState::Parked,
            pause: Arc::new(AtomicBool::new(false)),
        }))
    }

    pub fn from_parts(parts: SlotRuntimeParts) -> Self {
        Self {
            slot: parts.slot,
            bus: parts.bus,
            lapic: parts.lapic,
            entropy: parts.entropy,
            log: parts.log,
            machine_config: parts.machine_config,
            dirty_ring: parts.dirty_ring,
            dirty: parts.dirty,
            chain: parts.chain,
            counter: parts.counter,
            base_snapshot: parts.base_snapshot,
            position: parts.position,
            bisection_checkpoint_anchor_icount: parts.bisection_checkpoint_anchor_icount,
            queued_inputs: parts.queued_inputs,
            guest_events: parts.guest_events,
            next_input_order: parts.next_input_order,
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
    pub lapic: dh_vmm::lapic::LocalApic,
    pub entropy: dh_devices::entropy::DetEntropy,
    pub log: Option<dh_inputlog::dhilog::LogWriter>,
    pub machine_config: dh_vmm::config::MachineConfig,
    pub dirty_ring: dh_vmm::dirty::DirtyRing,
    pub dirty: dh_vmm::dirty::DirtyPageSet,
    pub chain: dh_vmm::hash::StateHashChain,
    pub counter: Option<dh_detclock::counter::InstRetired>,
    pub base_snapshot: Option<snapstore_types::SnapshotRef>,
    pub position: SlotPosition,
    pub bisection_checkpoint_anchor_icount: u64,
    pub queued_inputs: Vec<QueuedInput>,
    pub guest_events: Vec<DrainedGuestEvent>,
    pub next_input_order: u64,
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
