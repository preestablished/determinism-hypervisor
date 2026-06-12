//! M1 acceptance (bead 40q; IMPLEMENTATION-PLAN M1 accept): boot the
//! device-exercise guest with the REAL device surface wired into the
//! run loop — MmioBus (pv-clock, pv-pad, pv-entropy, pv-blk over the
//! ws4 base image) for MMIO exits, DetChannelHost for the detcall PIO
//! window, DebugSerial for the serial port — and assert every device
//! value end to end:
//!
//!   serial reads "CEPBDX" (every stage passed, in order);
//!   the channel doorbell drains exactly the guest's Beacon (0xB33F);
//!   channel mutations were logged (DHILOG record count grew);
//!   the guest's blk write landed in the OVERLAY only — the ws4 base
//!   file is byte-identical (BASE_IMAGE_BLAKE3) and mtime-untouched;
//!   MachineConfig carries the real base-image hash end to end;
//!   and the whole run is bit-identically repeatable (run twice).
//!
//! HARDWARE-GATED: kvm-intel lane + lab box; self-skips elsewhere.

// Everything here drives KVM (x86_64-only; bead v5w) - the whole test
// target compiles to empty on other arches.
#![cfg(target_arch = "x86_64")]

use std::io::ErrorKind;
use std::sync::atomic::AtomicBool;

use detguest_host::{GuestEvent, LogFaultPlan, OwnedPayload};
use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_devices::blk::PvBlk;
use dh_devices::bus::MmioBus;
use dh_devices::clock::{PvClock, PV_CLOCK_BASE};
use dh_devices::ctx::DevCtx;
use dh_devices::entropy::{DetEntropy, PvEntropy, PV_ENTROPY_BASE};
use dh_devices::pad::{PvPad, PV_PAD_BASE};
use dh_devices::serial::DebugSerial;
use dh_devices::DetChannelHost;
use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
use dh_vmm::blkfile::FileBase;
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::KvmSystem;
use dh_vmm::runctl::{run_segment, Segment, StopReason, Until};
use kvm_ioctls::VcpuExit;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const BLK_BASE_GPA: u64 = 0xD000_4000;
const DETCALL_LO: u16 = 0xD370;
const DETCALL_HI: u16 = 0xD3A0;
const ENTROPY_SEED: [u8; 32] = [0x4D; 32];

fn kvm_usable() -> bool {
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
    {
        Ok(_) => true,
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::PermissionDenied) => false,
        Err(e) => panic!("unexpected /dev/kvm probe failure: {e}"),
    }
}

/// The real guest RAM behind BOTH GuestMem seams (dh-devices DMA and
/// detguest-host channel access). GuestMemoryMmap is Arc-backed: clones
/// share the one mapping KVM runs against.
#[derive(Clone)]
struct VmMem(GuestMemoryMmap<()>);

impl dh_devices::ctx::GuestMem for VmMem {
    fn read(&self, gpa: u64, data: &mut [u8]) -> Result<(), dh_devices::ctx::MemError> {
        self.0
            .read_slice(data, GuestAddress(gpa))
            .map_err(|_| dh_devices::ctx::MemError)
    }
    fn write(&mut self, gpa: u64, data: &[u8]) -> Result<(), dh_devices::ctx::MemError> {
        self.0
            .write_slice(data, GuestAddress(gpa))
            .map_err(|_| dh_devices::ctx::MemError)
    }
}

impl detguest_host::GuestMem for VmMem {
    fn read(&self, gpa: u64, buf: &mut [u8]) -> Result<(), detguest_host::MemError> {
        self.0
            .read_slice(buf, GuestAddress(gpa))
            .map_err(|_| detguest_host::MemError::Unmapped {
                gpa,
                len: buf.len(),
            })
    }
    fn write(&mut self, gpa: u64, buf: &[u8]) -> Result<(), detguest_host::MemError> {
        self.0
            .write_slice(buf, GuestAddress(gpa))
            .map_err(|_| detguest_host::MemError::Unmapped {
                gpa,
                len: buf.len(),
            })
    }
}

struct RunOutcome {
    serial: Vec<u8>,
    beacons: Vec<GuestEvent>,
    log_records: u64,
    icount: u64,
    state_hash: String,
    /// Post-run device-model state (every bus device's snapshot section
    /// plus the detchannel host's): the run-twice comparison must cover
    /// state the RAM hash cannot see (overlay clusters, entropy stream
    /// position, channel seqs) — review iteration-48 I-1.
    device_state: Vec<u8>,
}

/// One cold boot of device_exercise over the full device surface.
/// `base_path` is the ws4 base image file (produced by the caller so it
/// can assert immutability afterwards).
fn run_m1(base_path: &std::path::Path) -> Result<RunOutcome, String> {
    dh_vmm::run::install_kick_handler().map_err(|e| format!("kick: {e}"))?;
    let sys = KvmSystem::open().map_err(|e| format!("{e:?}"))?;
    let mut slot = sys.create_slot_vm(16 << 20).map_err(|e| format!("{e:?}"))?;
    dh_vmm::boot::load_and_enter(&slot, nanokernel::device_exercise_elf(), b"")
        .map_err(|e| format!("{e}"))?;
    let counter = InstRetired::open_for_current_thread().map_err(|e| format!("{e:?}"))?;
    counter
        .route_overflow_to_thread(dh_vmm::run::current_tid(), dh_vmm::run::kick_signal())
        .map_err(|e| format!("{e:?}"))?;
    counter
        .arm_period(NEVER_FIRES_PERIOD)
        .map_err(|e| format!("{e:?}"))?;
    counter.reset().map_err(|e| format!("{e:?}"))?;
    counter.enable().map_err(|e| format!("{e:?}"))?;

    // MachineConfig plumbed end to end: the REAL ws4 base-image hash.
    let config = MachineConfig::new(
        16 << 20,
        nanokernel::image::BASE_IMAGE_BLAKE3,
        BootSpec::Elf {
            kernel_hash: [0xE1; 32],
            cmdline: Vec::new(),
        },
    );

    // ---- the device surface --------------------------------------------
    let vm_mem = VmMem(slot.guest_mem.clone());
    let mut bus = MmioBus::new();
    bus.register(PV_CLOCK_BASE, Box::new(PvClock::new(1, 1)))
        .map_err(|e| format!("{e:?}"))?;
    let mut pad = PvPad::new();
    // Host-injected latch (the guest's 'P' stage is a presence read).
    pad.apply_pad_set(0, 0x0000_A11B)
        .map_err(|e| format!("{e:?}"))?;
    bus.register(PV_PAD_BASE, Box::new(pad))
        .map_err(|e| format!("{e:?}"))?;
    bus.register(PV_ENTROPY_BASE, Box::new(PvEntropy::new()))
        .map_err(|e| format!("{e:?}"))?;
    let base = FileBase::open(base_path).map_err(|e| format!("{e}"))?;
    bus.register(BLK_BASE_GPA, Box::new(PvBlk::new(Box::new(base))))
        .map_err(|e| format!("{e:?}"))?;
    let mut channel: DetChannelHost<VmMem, LogFaultPlan> =
        DetChannelHost::new(vm_mem.clone(), LogFaultPlan::default());
    let mut entropy = DetEntropy::from_seed(ENTROPY_SEED);
    let mut log = LogWriter::new(SegmentHeader {
        base_snapshot_id: [0xA5; 32],
        entropy_seed: ENTROPY_SEED,
        machine_config_hash: config.config_hash().map_err(|e| format!("{e:?}"))?,
        clock_num: 1,
        clock_den: 1,
        // The real writer fingerprint (bead 4ld): per-segment header
        // field, a property of the encoder, present on restore paths too.
        encoder_fingerprint: dh_devices::detchannel::wire_encoder_fingerprint(),
    });
    let mut serial = DebugSerial::new();
    let mut beacons: Vec<GuestEvent> = Vec::new();
    let mut irqs = Vec::new();
    let mut dev_mem = vm_mem.clone();

    let mc_hash = config.config_hash().map_err(|e| format!("{e:?}"))?;
    let mut chain = StateHashChain::new(&mc_hash, &[0xA5; 32]);
    let pause = AtomicBool::new(false);
    let outcome = {
        let counter_ref = &counter;
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
        let log_r = &mut log;
        let beacons_r = &mut beacons;
        let irqs_r = &mut irqs;
        let dev_mem_r = &mut dev_mem;
        let bus_r = &mut bus;
        let channel_r = &mut channel;
        let entropy_r = &mut entropy;
        let serial_r = &mut serial;
        let mut on_exit = move |exit: VcpuExit| {
            // Per-dispatch device context. icount: the counter is stable
            // out of guest mode (§3.1). boundary_rip: not retrievable
            // inside on_exit (the vCPU is mutably borrowed by the
            // segment); 0 is the debug-loop convention.
            let icount = counter_ref
                .read()
                .map_err(|e| BoundaryError::Exit(format!("counter read: {e:?}")))?;
            let mut ctx = DevCtx::new(icount, 0, log_r, dev_mem_r, entropy_r, irqs_r);
            match exit {
                VcpuExit::IoOut(port, data) if (0x3F8..0x400).contains(&port) => {
                    serial_r.pio_write(port, data);
                }
                VcpuExit::IoIn(port, data) if (0x3F8..0x400).contains(&port) => {
                    serial_r.pio_read(port, data);
                }
                VcpuExit::IoOut(port, data) if (DETCALL_LO..DETCALL_HI).contains(&port) => {
                    let mut w = [0u8; 4];
                    w[..data.len().min(4)].copy_from_slice(&data[..data.len().min(4)]);
                    let events = channel_r.pio_out(port, u32::from_le_bytes(w), &mut ctx);
                    beacons_r.extend(events);
                }
                VcpuExit::IoIn(port, data) if (DETCALL_LO..DETCALL_HI).contains(&port) => {
                    let v = channel_r.pio_in(port, &mut ctx);
                    let b = v.to_le_bytes();
                    let n = data.len().min(4);
                    data[..n].copy_from_slice(&b[..n]);
                }
                VcpuExit::MmioRead(gpa, data) => {
                    bus_r
                        .read(gpa, data, &mut ctx)
                        .map_err(|e| BoundaryError::Exit(format!("bus read {gpa:#x}: {e:?}")))?;
                }
                VcpuExit::MmioWrite(gpa, data) => {
                    bus_r
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
        };
        run_segment(
            &mut seg,
            Until::IcountBudget(10_000_000),
            &mut || false,
            &mut on_exit,
        )
        .map_err(|e| format!("{e}"))?
    };
    if outcome.reason != StopReason::GuestHalted {
        return Err(format!("expected GuestHalted, got {:?}", outcome.reason));
    }
    // No device queued an interrupt: this guest never opens a window
    // (no sti/lidt), so a populated queue would mean a silently dropped
    // injection — lock it (review iteration-48 I-2).
    if !irqs.is_empty() {
        return Err(format!("undrained irq queue: {irqs:?}"));
    }
    // The channel really attached at the donated GPA (the guest's 'D'
    // proves it transitively; this proves it directly).
    if channel.channel_gpa() != Some(0x40_0000) {
        return Err(format!("channel not attached: {:?}", channel.channel_gpa()));
    }
    let mut device_state = Vec::new();
    for (base, dev) in bus.devices() {
        device_state.extend_from_slice(&base.to_le_bytes());
        device_state.extend_from_slice(&dev.device_id().to_le_bytes());
        dev.snapshot(&mut device_state);
    }
    channel.snapshot(&mut device_state);
    Ok(RunOutcome {
        serial: serial.take_output(),
        beacons,
        log_records: log.record_count(),
        icount: outcome.boundary.icount,
        state_hash: outcome
            .state_hash
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        device_state,
    })
}

#[test]
fn m1_device_exercise_end_to_end() {
    if !kvm_usable() {
        eprintln!("skipping: /dev/kvm not usable");
        return;
    }
    // ---- ws4 fixture production ----
    let base_path = std::env::temp_dir().join(format!("dh-m1-base-{}.img", std::process::id()));
    nanokernel::image::write_base_image(&base_path).unwrap();
    let mtime_before = std::fs::metadata(&base_path).unwrap().modified().unwrap();

    let out = run_m1(&base_path).expect("M1 run");

    // Every stage passed, in order.
    assert_eq!(
        out.serial,
        nanokernel::DEVICE_EXERCISE_OK_SEQUENCE,
        "serial log (a lowercase letter names the failed stage)"
    );

    // The doorbell drained exactly the guest's Beacon.
    assert_eq!(out.beacons.len(), 1, "exactly one channel event");
    match &out.beacons[0].payload {
        OwnedPayload::Beacon { beacon_id } => {
            assert_eq!(*beacon_id, nanokernel::DEVICE_EXERCISE_BEACON_ID);
        }
        other => panic!("expected Beacon, got {other:?}"),
    }

    // Channel mutations were logged: exactly 5 records, bit-stable
    // (review-measured: CHANNEL_INIT pio answers + doorbell DEV_EVENTs +
    // the entropy AUX record). The encoder fingerprint travels in the
    // segment HEADER (bead 4ld), not as a record.
    assert_eq!(out.log_records, 5, "DHILOG record count");

    // The guest's sector-0 write landed in the overlay ONLY.
    let meta = std::fs::metadata(&base_path).unwrap();
    assert_eq!(
        meta.modified().unwrap(),
        mtime_before,
        "base mtime must be untouched"
    );
    let on_disk = std::fs::read(&base_path).unwrap();
    assert_eq!(
        *blake3::hash(&on_disk).as_bytes(),
        nanokernel::image::BASE_IMAGE_BLAKE3,
        "base bytes must stay byte-identical"
    );

    // The full M1 surface is deterministic: cold-boot again, compare
    // everything observable.
    let out2 = run_m1(&base_path).expect("M1 second run");
    assert_eq!(
        (
            out.serial,
            out.icount,
            out.state_hash,
            out.log_records,
            out.device_state,
            format!("{:?}", out.beacons),
        ),
        (
            out2.serial,
            out2.icount,
            out2.state_hash,
            out2.log_records,
            out2.device_state,
            format!("{:?}", out2.beacons),
        ),
        "M1 run must be bit-identically repeatable, device state included"
    );

    std::fs::remove_file(&base_path).ok();
}
