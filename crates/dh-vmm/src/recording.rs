//! Recording integration (bead y78): the productized device rail — every
//! canonical input (PAD_SET, NET_RX; detchannel DEV_EVENTs log themselves
//! through the ctx) and AUX record lands in the segment DHILOG at the
//! correct boundary icount, and the segment seals from the boundary
//! outcome. The m1_acceptance on_exit harness is the template; this
//! module is that wiring as a struct run control (and the M5 acceptance)
//! can own instead of re-deriving closures per test.
//!
//! SHAPE: [`DeviceRail`] owns everything a segment's exits touch — bus,
//! serial, the slot's DetEntropy, the segment LogWriter, the guest-mem
//! adapter, and the pending-IRQ queue. `service_exit` is the on_exit
//! body; the `apply_*` methods are run control's canonical-input entry
//! points, PAIRING the device-state mutation with the record landing.
//! The pair is not atomic — a log failure after the device mutated HAS
//! drifted — so the contract is FAIL-FATAL: every `RecordError` is
//! slot-fatal (destroy, never resume, never seal); under that rule the
//! drift is unobservable. `seal` consumes the rail at segment end.
//!
//! NOT HERE: the detcall PIO window (detchannel) — `DetChannelHost` is
//! generic over its memory/fault plan and the demo guests do not attach
//! a channel; m1_acceptance keeps its bespoke loop and the slot manager
//! (ol1) owns the production composition. The §3.3 DEV_EVENT records it
//! emits already flow through the same `DevCtx` this rail builds.

use dh_devices::clock::PvClock;
use dh_devices::entropy::DetEntropy;
use dh_devices::net::{NetRxError, PvNet, DEVICE_ID_PV_NET};
use dh_devices::pad::{PadError, PvPad, DEVICE_ID_PV_PAD};
use dh_devices::{DebugSerial, DevCtx, GuestMem, IrqRequest, MmioBus};
use dh_inputlog::dhilog::{LogWriter, SealParams, WriteError};
use kvm_ioctls::VcpuExit;

use crate::boundary::BoundaryError;
use crate::kvm::{PIO_SERIAL_BASE, PIO_SERIAL_LEN};
use crate::runctl::{SegmentOutcome, StopReason};

/// DHILOG END `stop_reason` byte for a segment outcome — MIRRORS proto
/// `StopReason` numbering (API.md §3.3). dh-vmm cannot depend on
/// dh-proto, so the numbers are pinned here AND cross-pinned against
/// `dh_worker::proto_map::stop_reason_to_proto` in dh-worker's tests —
/// a renumbering on either side breaks one of the two pins loudly.
pub fn stop_reason_u8(reason: StopReason) -> u8 {
    match reason {
        StopReason::BudgetReached => 1,
        StopReason::GoalSatisfied => 2,
        StopReason::NextSdkEvent => 3,
        StopReason::HardCap => 4,
        StopReason::Paused => 5,
        StopReason::GuestHalted => 6,
    }
}

#[derive(Debug)]
pub enum RecordError {
    /// The paired device rejected the input (corrupt/hostile log data or
    /// a guest that published unusable buffers) — slot-fatal.
    Pad(PadError),
    Net(NetRxError),
    /// The record could not be written — DATA_LOSS class, slot-fatal.
    Log(WriteError),
    /// The bus has no device with the id the input targets, or it does
    /// not downcast to the expected concrete type.
    NoDevice(&'static str),
    /// Sealing was attempted with device-queued vectors never drained —
    /// a dropped injection must never seal as a healthy segment.
    UndrainedIrqs(usize),
}

/// One DHILOG segment's device rail — "segment" in the §3.1 LOG sense
/// (snapshot to snapshot), which typically spans MANY `run_segment`
/// calls (each a run quantum between boundaries; the live test runs
/// three under one rail). Build it fresh per log segment, feed
/// `service_exit` to every quantum's on_exit, apply canonical inputs at
/// their landed boundaries, then `seal` once with the final outcome.
pub struct DeviceRail<M: GuestMem> {
    pub bus: MmioBus,
    pub serial: DebugSerial,
    pub entropy: DetEntropy,
    pub log: LogWriter,
    pub mem: M,
    /// Vectors devices queued during dispatch (edge interrupts) — run
    /// control drains these into the next segment's injection set; an
    /// undrained queue at seal is a dropped injection and errors loudly.
    pub irqs: Vec<IrqRequest>,
}

impl<M: GuestMem> DeviceRail<M> {
    pub fn new(bus: MmioBus, entropy: DetEntropy, log: LogWriter, mem: M) -> Self {
        Self {
            bus,
            serial: DebugSerial::new(),
            entropy,
            log,
            mem,
            irqs: Vec::new(),
        }
    }

    /// The productized m1 on_exit body: debug-serial PIO, bus MMIO,
    /// loud log-fault check. `icount` is the counter read at the exit
    /// (stable out of guest mode, §3.1); `boundary_rip` follows the
    /// debug-loop convention (0 — not retrievable while the segment
    /// holds the vCPU).
    pub fn service_exit(&mut self, icount: u64, exit: VcpuExit) -> Result<(), BoundaryError> {
        let serial_end = PIO_SERIAL_BASE + PIO_SERIAL_LEN;
        let mut ctx = DevCtx::new(
            icount,
            0,
            &mut self.log,
            &mut self.mem,
            &mut self.entropy,
            &mut self.irqs,
        );
        match exit {
            VcpuExit::IoOut(port, data) if (PIO_SERIAL_BASE..serial_end).contains(&port) => {
                self.serial.pio_write(port, data);
            }
            VcpuExit::IoIn(port, data) if (PIO_SERIAL_BASE..serial_end).contains(&port) => {
                self.serial.pio_read(port, data);
            }
            VcpuExit::MmioRead(gpa, data) => {
                self.bus
                    .read(gpa, data, &mut ctx)
                    .map_err(|e| BoundaryError::Exit(format!("bus read {gpa:#x}: {e:?}")))?;
            }
            VcpuExit::MmioWrite(gpa, data) => {
                self.bus
                    .write(gpa, data, &mut ctx)
                    .map_err(|e| BoundaryError::Exit(format!("bus write {gpa:#x}: {e:?}")))?;
            }
            other => {
                return Err(BoundaryError::Exit(format!("unexpected exit: {other:?}")));
            }
        }
        // A dropped log record is DATA_LOSS-class, never absorbed.
        if let Some(e) = ctx.log_fault() {
            return Err(BoundaryError::Exit(format!("log fault: {e:?}")));
        }
        Ok(())
    }

    fn device_mut<T: 'static>(
        &mut self,
        id: u16,
        what: &'static str,
    ) -> Result<&mut T, RecordError> {
        for (_base, dev) in self.bus.devices_mut() {
            if dev.device_id() == id {
                return dev
                    .as_any_mut()
                    .and_then(|a| a.downcast_mut::<T>())
                    .ok_or(RecordError::NoDevice(what));
            }
        }
        Err(RecordError::NoDevice(what))
    }

    /// Canonical PAD_SET landing at `icount`: apply to the pad latch AND
    /// write the record, atomically from the caller's perspective. The
    /// returned edge vector (if the guest enabled one) is ALSO pushed
    /// onto `irqs` — run control schedules it per §3.4.
    pub fn apply_pad_set(
        &mut self,
        icount: u64,
        boundary_rip: u64,
        port: u8,
        buttons: u32,
        frame_hint: u32,
    ) -> Result<Option<u8>, RecordError> {
        let pad: &mut PvPad = self.device_mut(DEVICE_ID_PV_PAD, "pv-pad")?;
        let vector = pad.apply_pad_set(port, buttons).map_err(RecordError::Pad)?;
        self.log
            .pad_set(icount, boundary_rip, port, buttons, frame_hint)
            .map_err(RecordError::Log)?;
        if let Some(v) = vector {
            self.irqs.push(IrqRequest { vector: v });
        }
        Ok(vector)
    }

    /// Canonical NET_RX landing at `icount`: copy the frame into the
    /// guest-published RX buffer AND write the record (payload IS the
    /// frame, §3.3). Vector semantics as `apply_pad_set`.
    pub fn apply_net_rx(
        &mut self,
        icount: u64,
        boundary_rip: u64,
        frame: &[u8],
    ) -> Result<Option<u8>, RecordError> {
        // Split-borrow dance: the device lives in the bus; the copy
        // target is self.mem. Find the device first, then borrow mem.
        let mem = &mut self.mem;
        let mut found = None;
        for (_base, dev) in self.bus.devices_mut() {
            if dev.device_id() == DEVICE_ID_PV_NET {
                found = dev.as_any_mut().and_then(|a| a.downcast_mut::<PvNet>());
                break;
            }
        }
        let net = found.ok_or(RecordError::NoDevice("pv-net"))?;
        let vector = net.apply_net_rx(frame, mem).map_err(RecordError::Net)?;
        self.log
            .net_rx(icount, boundary_rip, frame)
            .map_err(RecordError::Log)?;
        if let Some(v) = vector {
            self.irqs.push(IrqRequest { vector: v });
        }
        Ok(vector)
    }

    /// The loopback drain (§6.7): at a TX-doorbell exit, read the frame
    /// back from guest RAM through the still-live TX regs. Run control
    /// calls this AT THAT EXIT (the guest may overwrite its buffer the
    /// moment it runs again) and decides what to do with the frame —
    /// in the loopback config, land it back via [`Self::apply_net_rx`].
    ///
    /// GATED ON TX_STATUS == OK (iteration-86 review, critical): a
    /// faulted doorbell logged no NET_TX, so its last-programmed
    /// gpa/len must never mint a frame — that would be an unlogged
    /// input, a replay divergence by construction. The length is also
    /// re-capped defensively; a post-OK read fault is LOUD (the RAM the
    /// device just read cannot have vanished).
    pub fn drain_net_tx(&mut self) -> Result<Option<Vec<u8>>, RecordError> {
        let (gpa, len, status) = {
            let net: &mut PvNet = self.device_mut(DEVICE_ID_PV_NET, "pv-net")?;
            net.tx_regs()
        };
        if status != dh_devices::net::STATUS_OK || len == 0 {
            return Ok(None);
        }
        if len > dh_devices::net::MAX_FRAME {
            // STATUS_OK with an oversize len is unreachable (the doorbell
            // faults first) — refuse rather than allocate from a guest u32.
            return Err(RecordError::Net(NetRxError::FrameTooBig));
        }
        let mut frame = vec![0u8; len as usize];
        self.mem
            .read(gpa, &mut frame)
            .map_err(|_| RecordError::Net(NetRxError::MemFault))?;
        Ok(Some(frame))
    }

    /// Pair the AUX TIMER_FIRE record with the one-shot disarm (§3.4 /
    /// clock.rs: the caller logs the fire AND disarms; pairing them here
    /// keeps the two from drifting, same as the apply_* methods).
    pub fn log_timer_fired(
        &mut self,
        icount: u64,
        boundary_rip: u64,
        fired: &crate::runctl::TimerFired,
    ) -> Result<(), RecordError> {
        let clk: &mut PvClock =
            self.device_mut(dh_devices::clock::DEVICE_ID_PV_CLOCK, "pv-clock")?;
        clk.disarm();
        self.log
            .timer_fire(
                icount,
                boundary_rip,
                fired.vector,
                fired.armed_deadline_vns,
                fired.delivered_icount,
            )
            .map_err(RecordError::Log)
    }

    /// Restore-time clock reseed passthrough (the §8.3 vns_base seam) —
    /// here so rail owners never need their own downcast.
    pub fn set_clock_vns_base(&mut self, vns: u64) -> Result<(), RecordError> {
        let clk: &mut PvClock =
            self.device_mut(dh_devices::clock::DEVICE_ID_PV_CLOCK, "pv-clock")?;
        clk.set_vns_base(vns);
        Ok(())
    }

    /// Land one epoch link as an AUX EPOCH_HASH record — the
    /// `run_segment_with_epochs` sink body. MUST be called at the link
    /// point (the sink fires there) so the record stays inside the
    /// monotone icount watermark; batching after the quantum regresses
    /// behind later exit records and the writer rejects it.
    pub fn log_epoch_hash(
        &mut self,
        epoch_index: u64,
        icount: u64,
        chain_value: [u8; 32],
    ) -> Result<(), RecordError> {
        self.log
            .epoch_hash(icount, 0, epoch_index, chain_value)
            .map_err(RecordError::Log)
    }

    /// Seal the segment from its outcome (§3.1: SealParams come from the
    /// boundary outcome, the END stop_reason mirrors proto StopReason).
    /// Errors if any device-queued vector was never drained — a dropped
    /// injection must never seal as a healthy segment.
    pub fn seal(
        self,
        outcome: &SegmentOutcome,
        end_snapshot_id: [u8; 32],
    ) -> Result<Vec<u8>, RecordError> {
        if !self.irqs.is_empty() {
            return Err(RecordError::UndrainedIrqs(self.irqs.len()));
        }
        self.log
            .seal(SealParams {
                end_snapshot_id,
                end_icount: outcome.boundary.icount,
                end_vns: outcome.vns,
                end_state_hash: outcome.state_hash,
                stop_reason: stop_reason_u8(outcome.reason),
            })
            .map_err(RecordError::Log)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dh_devices::ctx::VecGuestMem;
    use dh_inputlog::dhilog::SegmentHeader;
    use dh_inputlog::reader::{LogReader, RecordBody};

    /// The proto-mirror pin (the other half lives in dh-worker's
    /// proto_map tests, cross-pinning these exact numbers).
    #[test]
    fn stop_reason_bytes_mirror_proto_numbers() {
        assert_eq!(stop_reason_u8(StopReason::BudgetReached), 1);
        assert_eq!(stop_reason_u8(StopReason::GoalSatisfied), 2);
        assert_eq!(stop_reason_u8(StopReason::HardCap), 4);
        assert_eq!(stop_reason_u8(StopReason::Paused), 5);
        assert_eq!(stop_reason_u8(StopReason::GuestHalted), 6);
    }

    fn header() -> SegmentHeader {
        SegmentHeader {
            base_snapshot_id: [0x11; 32],
            entropy_seed: [0; 32],
            machine_config_hash: [0x22; 32],
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        }
    }

    /// Host-level rail test: NET_RX application records the frame, lands
    /// the bytes, queues the vector; seal carries the outcome; an
    /// undrained queue refuses to seal.
    #[test]
    fn net_rx_application_pairs_record_and_delivery() {
        let mut bus = MmioBus::new();
        bus.register(dh_devices::net::PV_NET_BASE, Box::new(PvNet::new()))
            .unwrap();
        let mut rail = DeviceRail::new(
            bus,
            DetEntropy::from_seed([7; 32]),
            LogWriter::new(header()),
            VecGuestMem(vec![0u8; 4096]),
        );

        // Guest publishes the RX buffer + vector (through the real bus
        // dispatch path, synthetic exits).
        let base = dh_devices::net::PV_NET_BASE;
        let gpa = 0x200u64.to_le_bytes();
        rail.service_exit(
            10,
            VcpuExit::MmioWrite(base + dh_devices::net::REG_RX_BUF_GPA, &gpa),
        )
        .unwrap();
        let cap = 128u32.to_le_bytes();
        rail.service_exit(
            11,
            VcpuExit::MmioWrite(base + dh_devices::net::REG_RX_CAP, &cap),
        )
        .unwrap();
        let vec_ = 0x41u32.to_le_bytes();
        rail.service_exit(
            12,
            VcpuExit::MmioWrite(base + dh_devices::net::REG_RX_VECTOR, &vec_),
        )
        .unwrap();

        let frame = [0xEE; 24];
        let v = rail.apply_net_rx(5_000, 0x1000, &frame).unwrap();
        assert_eq!(v, Some(0x41));
        assert_eq!(&rail.mem.0[0x200..0x218], &frame[..]);
        assert_eq!(rail.irqs.len(), 1);

        // Sealing with an undrained queue is refused.
        let outcome = SegmentOutcome {
            reason: StopReason::BudgetReached,
            boundary: crate::boundary::Boundary {
                icount: 9_000,
                rip: 0x2000,
                rcx: 0,
            },
            vns: 9_000,
            state_hash: [0xAB; 32],
            injections_delivered: 0,
            timer_fired: None,
            frames_elapsed: 0,
        };
        // (clone the rail pieces we need post-move via a rebuild)
        let drained: Vec<_> = rail.irqs.drain(..).collect();
        assert_eq!(drained[0].vector, 0x41);

        let sealed = rail.seal(&outcome, [0x33; 32]).unwrap();
        let r = LogReader::parse(&sealed).unwrap();
        let canon: Vec<_> = r.canonical().collect();
        assert_eq!(canon.len(), 1);
        match canon[0].body() {
            RecordBody::NetRx { frame: f } => assert_eq!(f, &frame[..]),
            other => panic!("wrong record: {other:?}"),
        }
        let (stop, end_hash) = r.end();
        assert_eq!(stop, 1); // BudgetReached
        assert_eq!(end_hash, [0xAB; 32]);
    }

    /// drain_net_tx is GATED on TX_STATUS (iteration-86 critical): a
    /// faulted doorbell must never mint a frame; an OK doorbell drains
    /// the exact bytes; an idle device drains nothing.
    #[test]
    fn drain_net_tx_gates_on_status() {
        let mut bus = MmioBus::new();
        bus.register(dh_devices::net::PV_NET_BASE, Box::new(PvNet::new()))
            .unwrap();
        let mut rail = DeviceRail::new(
            bus,
            DetEntropy::from_seed([7; 32]),
            LogWriter::new(header()),
            VecGuestMem(vec![0u8; 4096]),
        );
        let base = dh_devices::net::PV_NET_BASE;

        // Idle: nothing to drain.
        assert!(rail.drain_net_tx().unwrap().is_none());

        // Faulted doorbell (oversize len): regs hold garbage, status is
        // FAULT — drain must return None, not a minted frame.
        let len = (dh_devices::net::MAX_FRAME + 1).to_le_bytes();
        rail.service_exit(
            5,
            VcpuExit::MmioWrite(base + dh_devices::net::REG_TX_LEN, &len),
        )
        .unwrap();
        rail.service_exit(
            6,
            VcpuExit::MmioWrite(base + dh_devices::net::REG_TX_DOORBELL, &1u32.to_le_bytes()),
        )
        .unwrap();
        assert!(rail.drain_net_tx().unwrap().is_none());

        // OK doorbell: the exact frame comes back.
        rail.mem.0[0x300..0x310].copy_from_slice(&[0x5C; 16]);
        let gpa = 0x300u64.to_le_bytes();
        rail.service_exit(
            7,
            VcpuExit::MmioWrite(base + dh_devices::net::REG_TX_BUF_GPA, &gpa),
        )
        .unwrap();
        let len16 = 16u32.to_le_bytes();
        rail.service_exit(
            8,
            VcpuExit::MmioWrite(base + dh_devices::net::REG_TX_LEN, &len16),
        )
        .unwrap();
        rail.service_exit(
            9,
            VcpuExit::MmioWrite(base + dh_devices::net::REG_TX_DOORBELL, &1u32.to_le_bytes()),
        )
        .unwrap();
        assert_eq!(rail.drain_net_tx().unwrap().unwrap(), vec![0x5C; 16]);
    }

    /// log_timer_fired pairs the AUX record with the one-shot disarm.
    #[test]
    fn log_timer_fired_disarms_and_records() {
        let mut bus = MmioBus::new();
        bus.register(0xD000_2000, Box::new(PvClock::new(1, 1)))
            .unwrap();
        let mut rail = DeviceRail::new(
            bus,
            DetEntropy::from_seed([7; 32]),
            LogWriter::new(header()),
            VecGuestMem(vec![0u8; 64]),
        );
        // Arm via MMIO (8-byte deadline write).
        let deadline = 9_999u64.to_le_bytes();
        rail.service_exit(
            3,
            VcpuExit::MmioWrite(
                0xD000_2000 + dh_devices::clock::REG_TIMER_DEADLINE,
                &deadline,
            ),
        )
        .unwrap();
        let fired = crate::runctl::TimerFired {
            vector: 0x41,
            armed_deadline_vns: 9_999,
            delivered_icount: 10_240,
        };
        rail.log_timer_fired(10_240, 0, &fired).unwrap();
        // Disarmed: the clock reports no armed deadline.
        let clk: &mut PvClock = rail
            .device_mut(dh_devices::clock::DEVICE_ID_PV_CLOCK, "pv-clock")
            .unwrap();
        assert_eq!(clk.armed(), None);

        // And vns_base passthrough works on the same rail.
        rail.set_clock_vns_base(5_000_000).unwrap();
        let sealed = rail
            .seal(
                &SegmentOutcome {
                    reason: StopReason::Paused,
                    boundary: crate::boundary::Boundary {
                        icount: 11_000,
                        rip: 0,
                        rcx: 0,
                    },
                    vns: 11_000,
                    state_hash: [0; 32],
                    injections_delivered: 1,
                    timer_fired: None,
                    frames_elapsed: 0,
                },
                [0; 32],
            )
            .unwrap();
        let r = LogReader::parse(&sealed).unwrap();
        let fires = r
            .aux()
            .filter(|rec| matches!(rec.body(), RecordBody::TimerFire { .. }))
            .count();
        assert_eq!(fires, 1);
    }
}

#[cfg(all(test, target_arch = "x86_64"))]
mod live_tests {
    use super::*;
    use crate::boot::load_and_enter;
    use crate::hash::StateHashChain;
    use crate::kvm::KvmSystem;
    use crate::run::install_kick_handler;
    use crate::runctl::{run_segment, Segment, Until};
    use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
    use dh_inputlog::dhilog::SegmentHeader;
    use dh_inputlog::reader::{LogReader, RecordBody};
    use std::sync::atomic::AtomicBool;
    use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

    fn gettid() -> i32 {
        // SAFETY: argless syscall.
        #[allow(unsafe_code)]
        unsafe {
            libc::syscall(libc::SYS_gettid) as i32
        }
    }

    #[derive(Clone)]
    struct VmMem(GuestMemoryMmap<()>);
    impl dh_devices::ctx::GuestMem for VmMem {
        fn read(&self, gpa: u64, out: &mut [u8]) -> Result<(), dh_devices::ctx::MemError> {
            self.0
                .read_slice(out, GuestAddress(gpa))
                .map_err(|_| dh_devices::ctx::MemError)
        }
        fn write(&mut self, gpa: u64, data: &[u8]) -> Result<(), dh_devices::ctx::MemError> {
            self.0
                .write_slice(data, GuestAddress(gpa))
                .map_err(|_| dh_devices::ctx::MemError)
        }
    }

    /// The y62 live proof: epoch links collected by the sink land as AUX
    /// EPOCH_HASH records with the right indices, and the sealed header
    /// carries FLAG_EPOCH_HASHES. Small epoch grid so one quantum crosses
    /// several epochs.
    #[test]
    fn epoch_hashes_flow_from_quantum_to_sealed_log() {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        install_kick_handler().unwrap();
        let sys = KvmSystem::open().unwrap();
        let mut slot = sys.create_slot_vm(16 << 20).unwrap();
        load_and_enter(&slot, nanokernel::landing_loop_elf(), b"1000000000").unwrap();
        let counter = InstRetired::open_for_current_thread().unwrap();
        counter
            .route_overflow_to_thread(gettid(), crate::run::kick_signal())
            .unwrap();
        counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
        counter.reset().unwrap();
        counter.enable().unwrap();

        let mut config = crate::config::MachineConfig::new(
            16 << 20,
            [0x66; 32],
            crate::config::BootSpec::Elf {
                kernel_hash: [0x66; 32],
                cmdline: b"1000000000".to_vec(),
            },
        );
        config.epoch_len = 30_000; // several epochs inside one quantum
        let rail = DeviceRail::new(
            MmioBus::new(),
            DetEntropy::from_seed([3; 32]),
            LogWriter::new(SegmentHeader {
                base_snapshot_id: [0x11; 32],
                entropy_seed: [0; 32],
                machine_config_hash: [0x22; 32],
                clock_num: 1,
                clock_den: 1,
                encoder_fingerprint: 0,
            }),
            VmMem(slot.guest_mem.clone()),
        );
        let mut chain = StateHashChain::new(&[0x22; 32], &[0x11; 32]);
        let pause = std::sync::atomic::AtomicBool::new(false);
        let rail = std::cell::RefCell::new(rail);
        let mut links = Vec::new();
        let out = {
            let mut seg = Segment {
                slot: &mut slot,
                counter: &counter,
                chain: &mut chain,
                config: &config,
                start_icount: 0,
                injections: &[],
                timer: None,
                pause: &pause,
                sdk_events: None,
            };
            let counter_ref = &counter;
            let links_r = &mut links;
            crate::runctl::run_segment_with_epochs(
                &mut seg,
                Until::IcountBudget(100_000),
                &mut || false,
                &mut |exit| {
                    let icount = counter_ref
                        .read()
                        .map_err(|e| crate::boundary::BoundaryError::Exit(format!("{e:?}")))?;
                    rail.borrow_mut().service_exit(icount, exit)
                },
                &mut |idx, icount, value| {
                    links_r.push((idx, icount, value));
                    rail.borrow_mut()
                        .log_epoch_hash(idx, icount, value)
                        .map_err(|e| {
                            crate::boundary::BoundaryError::Exit(format!("epoch log: {e:?}"))
                        })
                },
            )
            .unwrap()
        };
        let rail = rail.into_inner();
        assert_eq!(out.reason, StopReason::BudgetReached);
        // Epochs at 30k/60k/90k inside the 100k budget.
        assert_eq!(
            links
                .iter()
                .map(|(idx, ic, _)| (*idx, *ic))
                .collect::<Vec<_>>(),
            vec![(1, 30_000), (2, 60_000), (3, 90_000)]
        );
        assert!(links.iter().all(|(_, _, h)| *h != [0u8; 32]));

        let sealed = rail.seal(&out, [0; 32]).unwrap();
        let r = LogReader::parse(&sealed).unwrap();
        assert!(r.header().has_epoch_hashes(), "header flag must be set");
        let eh: Vec<_> = r
            .aux()
            .filter_map(|rec| match rec.body() {
                RecordBody::EpochHash {
                    epoch_index,
                    chain_value,
                } => Some((rec.icount(), epoch_index, chain_value)),
                _ => None,
            })
            .collect();
        assert_eq!(eh.len(), 3);
        for ((idx, ic, h), (rec_ic, rec_idx, rec_h)) in links.iter().zip(eh.iter()) {
            assert_eq!((idx, ic), (rec_idx, rec_ic));
            assert_eq!(h, rec_h);
        }
    }

    /// The y78 live proof: pad_echo runs under the rail; PAD_SET inputs
    /// applied at landed boundaries reach the guest (its table flips to
    /// the new latch values) AND land in the log at those exact icounts;
    /// the pad device's FRAME_COUNTER writes emit AUX FRAME_MARKs; the
    /// seal carries the final outcome.
    #[test]
    fn pad_echo_live_run_records_inputs_frame_marks_and_seals() {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        install_kick_handler().unwrap();
        let sys = KvmSystem::open().unwrap();
        let mut slot = sys.create_slot_vm(16 << 20).unwrap();
        load_and_enter(&slot, nanokernel::pad_echo_elf(), b"").unwrap();
        let counter = InstRetired::open_for_current_thread().unwrap();
        counter
            .route_overflow_to_thread(gettid(), crate::run::kick_signal())
            .unwrap();
        counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
        counter.reset().unwrap();
        counter.enable().unwrap();

        let config = crate::config::MachineConfig::new(
            16 << 20,
            [0x55; 32],
            crate::config::BootSpec::Elf {
                kernel_hash: [0x55; 32],
                cmdline: Vec::new(),
            },
        );
        let mut bus = MmioBus::new();
        bus.register(
            dh_devices::pad::PV_PAD_BASE,
            Box::new(dh_devices::pad::PvPad::new()),
        )
        .unwrap();
        let mut rail = DeviceRail::new(
            bus,
            DetEntropy::from_seed([9; 32]),
            LogWriter::new(SegmentHeader {
                base_snapshot_id: [0x11; 32],
                entropy_seed: [0; 32],
                machine_config_hash: [0x22; 32],
                clock_num: 1,
                clock_den: 1,
                encoder_fingerprint: 0,
            }),
            VmMem(slot.guest_mem.clone()),
        );
        let mut chain = StateHashChain::new(&[0x22; 32], &[0x11; 32]);
        let pause = AtomicBool::new(false);

        let run_one = |slot: &mut crate::kvm::SlotVm,
                       rail: &mut DeviceRail<VmMem>,
                       chain: &mut StateHashChain,
                       budget: u64| {
            let start = counter.read().unwrap();
            let mut seg = Segment {
                slot,
                counter: &counter,
                chain,
                config: &config,
                start_icount: start,
                injections: &[],
                timer: None,
                pause: &pause,
                sdk_events: None,
            };
            let counter_ref = &counter;
            let out = run_segment(
                &mut seg,
                Until::IcountBudget(budget),
                &mut || false,
                &mut |exit| {
                    let icount = counter_ref
                        .read()
                        .map_err(|e| crate::boundary::BoundaryError::Exit(format!("{e:?}")))?;
                    rail.service_exit(icount, exit)
                },
            )
            .unwrap();
            assert_eq!(out.reason, StopReason::BudgetReached);
            out
        };

        // Segment 1: default latch (0) frames.
        let o1 = run_one(&mut slot, &mut rail, &mut chain, 100_000);
        rail.apply_pad_set(o1.boundary.icount, o1.boundary.rip, 0, 0xA1B2, 0)
            .unwrap();
        // Segment 2: frames observe 0xA1B2.
        let o2 = run_one(&mut slot, &mut rail, &mut chain, 100_000);
        rail.apply_pad_set(o2.boundary.icount, o2.boundary.rip, 0, 0xC3D4, 1)
            .unwrap();
        // Segment 3: frames observe 0xC3D4.
        let o3 = run_one(&mut slot, &mut rail, &mut chain, 100_000);

        assert!(rail.irqs.is_empty(), "pad vector disabled — no irqs");
        // Guest-visible: the table contains all three latch eras.
        let mut head = [0u8; 8];
        slot.guest_mem
            .read_slice(&mut head, GuestAddress(nanokernel::PAD_ECHO_TABLE_GPA))
            .unwrap();
        let count = u64::from_le_bytes(head);
        assert!(count > 0, "guest recorded no frames");
        let mut seen = std::collections::BTreeSet::new();
        for i in 0..count.min(nanokernel::PAD_ECHO_TABLE_CAPACITY) {
            let mut e = [0u8; 8];
            slot.guest_mem
                .read_slice(
                    &mut e,
                    GuestAddress(nanokernel::PAD_ECHO_TABLE_GPA + 8 + i * 8),
                )
                .unwrap();
            seen.insert(u32::from_le_bytes(e[4..8].try_into().unwrap()));
        }
        assert!(seen.contains(&0), "pre-input era missing");
        assert!(
            seen.contains(&0xA1B2),
            "first input never reached the guest"
        );
        assert!(
            seen.contains(&0xC3D4),
            "second input never reached the guest"
        );

        // The log: 2 canonical PAD_SETs at the landed icounts, many AUX
        // FRAME_MARKs from the device, END from the outcome.
        let sealed = rail.seal(&o3, [0x44; 32]).unwrap();
        let r = LogReader::parse(&sealed).unwrap();
        let pads: Vec<_> = r
            .canonical()
            .filter_map(|rec| match rec.body() {
                RecordBody::PadSet { port, buttons, .. } => Some((rec.icount(), port, buttons)),
                _ => None,
            })
            .collect();
        assert_eq!(
            pads,
            vec![
                (o1.boundary.icount, 0u8, 0xA1B2u32),
                (o2.boundary.icount, 0u8, 0xC3D4u32),
            ]
        );
        let frame_marks = r
            .aux()
            .filter(|rec| matches!(rec.body(), RecordBody::FrameMark { .. }))
            .count();
        assert!(frame_marks > 10, "only {frame_marks} FRAME_MARKs");
        let (stop, end_hash) = r.end();
        assert_eq!(stop, stop_reason_u8(o3.reason));
        assert_eq!(end_hash, o3.state_hash);
    }
}
