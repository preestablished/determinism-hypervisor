//! Direct Linux guest harness for dh-cli.
//!
//! This path deliberately uses dh-vmm and dh-devices directly. It does not
//! depend on dh-worker's image cache or gRPC runtime machinery.

use std::cell::{Cell, RefCell};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_devices::detchannel::DetChannelDevice;
use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::KvmSystem;
use dh_vmm::runctl::{run_segment_with_options, RunOptions, Segment, StopReason, Until};
use kvm_ioctls::VcpuExit;

pub const DEFAULT_LINUX_MEM_BYTES: u64 = 128 * 1024 * 1024;
pub const DEFAULT_READY_HARD_CAP: u64 = 10_000_000_000;
pub const READY_EVENT_KIND: u16 = detguest_wire::record::EventKind::Ready as u16;

const DETCHANNEL_MMIO_BASE: u64 = 0xD000_3000;
const PV_BLK_MMIO_BASE: u64 = 0xD000_4000;
const DEBUG_SERIAL_MMIO_BASE: u64 = 0xD000_6000;
const ENTROPY_SEED: [u8; 32] = [0x9A; 32];

type CliDetChannel = DetChannelDevice<
    RuntimeVmMem,
    detguest_host::LogFaultPlan,
    fn() -> detguest_host::LogFaultPlan,
>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinuxGuestPaths {
    pub bzimage: PathBuf,
    pub initramfs: PathBuf,
    pub base_image: PathBuf,
    pub game_image: PathBuf,
    pub cmdline_extra: Vec<String>,
    pub mem_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinuxReadyReport {
    pub ready_event_kind: u16,
    pub ready_payload_len: usize,
    pub ready_unit: u32,
    pub ready_region_count: u32,
    pub ready_manifest_generation: u64,
    pub ready_payload_digest: String,
    pub reason: &'static str,
    pub icount: u64,
    pub vns: u64,
    pub state_hash: String,
    pub config_hash: String,
    pub bzimage_hash: String,
    pub initramfs_hash: String,
    pub base_image_hash: String,
    pub game_image_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadyPayload {
    pub unit: u32,
    pub region_count: u32,
    pub manifest_generation: u64,
    pub digest: [u8; 32],
}

#[derive(Clone)]
struct RuntimeVmMem(vm_memory::GuestMemoryMmap<()>);

impl dh_devices::ctx::GuestMem for RuntimeVmMem {
    fn read(&self, gpa: u64, out: &mut [u8]) -> Result<(), dh_devices::ctx::MemError> {
        use vm_memory::Bytes;
        self.0
            .read_slice(out, vm_memory::GuestAddress(gpa))
            .map_err(|_| dh_devices::ctx::MemError)
    }

    fn write(&mut self, gpa: u64, data: &[u8]) -> Result<(), dh_devices::ctx::MemError> {
        use vm_memory::Bytes;
        self.0
            .write_slice(data, vm_memory::GuestAddress(gpa))
            .map_err(|_| dh_devices::ctx::MemError)
    }
}

impl detguest_host::GuestMem for RuntimeVmMem {
    fn read(&self, gpa: u64, out: &mut [u8]) -> Result<(), detguest_host::MemError> {
        if gpa.checked_add(out.len() as u64).is_none() {
            return Err(detguest_host::MemError::Overflow);
        }
        use vm_memory::Bytes;
        self.0
            .read_slice(out, vm_memory::GuestAddress(gpa))
            .map_err(|_| detguest_host::MemError::Unmapped {
                gpa,
                len: out.len(),
            })
    }

    fn write(&mut self, gpa: u64, data: &[u8]) -> Result<(), detguest_host::MemError> {
        if gpa.checked_add(data.len() as u64).is_none() {
            return Err(detguest_host::MemError::Overflow);
        }
        use vm_memory::Bytes;
        self.0
            .write_slice(data, vm_memory::GuestAddress(gpa))
            .map_err(|_| detguest_host::MemError::Unmapped {
                gpa,
                len: data.len(),
            })
    }
}

pub fn run_to_ready(
    paths: &LinuxGuestPaths,
    hard_cap: u64,
    paranoid_hash: bool,
) -> Result<LinuxReadyReport, String> {
    dh_vmm::run::install_kick_handler().map_err(|e| format!("kick handler: {e}"))?;

    let bzimage = read_file(&paths.bzimage)?;
    let initramfs = read_file(&paths.initramfs)?;
    let bzimage_hash = hash_bytes(&bzimage);
    let initramfs_hash = hash_bytes(&initramfs);
    let base_image_hash = hash_file(&paths.base_image)?;
    let game_image_hash = hash_file(&paths.game_image)?;
    let cmdline = linux_cmdline(paths)?;

    let sys = KvmSystem::open().map_err(|e| format!("KVM unavailable: {e:?}"))?;
    if !sys.dirty_ring {
        return Err("KVM dirty ring unavailable".into());
    }
    let mut slot = sys
        .create_slot_vm(paths.mem_bytes)
        .map_err(|e| format!("create slot VM: {e:?}"))?;

    let mut config = MachineConfig::new(
        paths.mem_bytes,
        game_image_hash,
        BootSpec::BzImage {
            kernel_hash: bzimage_hash,
            initramfs_hash,
            cmdline: cmdline.clone(),
        },
    );
    config.cpuid_table = slot.cpuid_table.clone();
    config.device_set = vec![
        dh_devices::detchannel::DEVICE_ID_DETCHANNEL,
        dh_devices::clock::DEVICE_ID_PV_CLOCK,
        dh_devices::pad::DEVICE_ID_PV_PAD,
        dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
        dh_devices::blk::DEVICE_ID_PV_BLK,
        dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
    ];
    config
        .validate()
        .map_err(|e| format!("MachineConfig invalid: {e:?}"))?;
    let config_hash = config
        .config_hash()
        .map_err(|e| format!("MachineConfig hash: {e:?}"))?;

    dh_vmm::boot::load_bzimage_and_enter(&slot, &bzimage, &initramfs, &cmdline)
        .map_err(|e| format!("BzImage boot: {e}"))?;

    let counter = InstRetired::open_for_current_thread().map_err(|e| format!("{e:?}"))?;
    counter
        .route_overflow_to_thread(dh_vmm::run::current_tid(), dh_vmm::run::kick_signal())
        .map_err(|e| format!("{e:?}"))?;
    counter
        .arm_period(NEVER_FIRES_PERIOD)
        .map_err(|e| format!("{e:?}"))?;
    counter.reset().map_err(|e| format!("{e:?}"))?;
    counter.enable().map_err(|e| format!("{e:?}"))?;

    let mut chain = StateHashChain::new(&config_hash, &[0; 32]);
    let pause = AtomicBool::new(false);
    let log = LogWriter::new(SegmentHeader {
        base_snapshot_id: [0; 32],
        entropy_seed: ENTROPY_SEED,
        machine_config_hash: config_hash,
        clock_num: config.clock.num(),
        clock_den: config.clock.den(),
        encoder_fingerprint: 0,
    });
    let bus = build_linux_bus(
        &config,
        dh_vmm::blkfile::FileBase::open(&paths.game_image)
            .map_err(|e| format!("open game image {}: {e}", paths.game_image.display()))?,
        RuntimeVmMem(slot.guest_mem.clone()),
    )?;
    let rail = RefCell::new(dh_vmm::recording::DeviceRail::new(
        bus,
        dh_devices::entropy::DetEntropy::from_seed(ENTROPY_SEED),
        log,
        RuntimeVmMem(slot.guest_mem.clone()),
    ));
    let ready_feed = Cell::new(0u64);
    let mut ready_event: Option<ReadyPayload> = None;

    let outcome = {
        let hash_device_sections = || {
            let rail_ref = rail.borrow();
            linux_hash_device_sections(&rail_ref.bus, &rail_ref.lapic)
        };
        let mut on_exit = |exit: VcpuExit<'_>| {
            let icount = counter
                .read()
                .map_err(|e| BoundaryError::Exit(format!("counter read: {e:?}")))?;
            let events = service_linux_exit(&mut rail.borrow_mut(), icount, exit)?;
            for event in &events {
                if let Some((kind, payload)) =
                    dh_devices::detchannel::stream_guest_event_payload(event)
                {
                    if kind == READY_EVENT_KIND {
                        let ready = parse_ready_payload(&payload)?;
                        ready_feed.set(ready_feed.get() + 1);
                        ready_event.get_or_insert(ready);
                    }
                }
            }
            Ok(())
        };
        let mut segment = Segment {
            slot: &mut slot,
            counter: &counter,
            chain: &mut chain,
            config: &config,
            start_icount: 0,
            injections: &[],
            timer: None,
            pause: &pause,
            sdk_events: Some(&ready_feed),
            hash_device_sections: Some(&hash_device_sections),
        };
        run_segment_with_options(
            &mut segment,
            Until::NextSdkEvent { hard_cap },
            RunOptions {
                paranoid_hash,
                ..RunOptions::default()
            },
            &mut || false,
            &mut on_exit,
        )
        .map_err(|e| format!("{e}"))?
    };
    counter.disable().map_err(|e| format!("{e:?}"))?;

    let ready = ready_event
        .ok_or_else(|| format!("run stopped without Ready EventKind {READY_EVENT_KIND}"))?;
    if outcome.reason != StopReason::NextSdkEvent {
        return Err(format!(
            "run stopped with {:?}, expected NextSdkEvent",
            outcome.reason
        ));
    }

    Ok(LinuxReadyReport {
        ready_event_kind: READY_EVENT_KIND,
        ready_payload_len: READY_PAYLOAD_LEN,
        ready_unit: ready.unit,
        ready_region_count: ready.region_count,
        ready_manifest_generation: ready.manifest_generation,
        ready_payload_digest: hex(&ready.digest),
        reason: "next_sdk_event",
        icount: outcome.boundary.icount,
        vns: outcome.vns,
        state_hash: hex(&outcome.state_hash),
        config_hash: hex(&config_hash),
        bzimage_hash: hex(&bzimage_hash),
        initramfs_hash: hex(&initramfs_hash),
        base_image_hash: hex(&base_image_hash),
        game_image_hash: hex(&game_image_hash),
    })
}

pub fn ready_fingerprint(paths: &LinuxGuestPaths, hard_cap: u64) -> Result<String, String> {
    let r = run_to_ready(paths, hard_cap, false)?;
    Ok(format!(
        "ready_event_kind={} ready_unit={} ready_region_count={} ready_manifest_generation={} ready_payload_digest={} icount={} vns={} state_hash={} config_hash={} game_image_hash={} base_image_hash={}",
        r.ready_event_kind,
        r.ready_unit,
        r.ready_region_count,
        r.ready_manifest_generation,
        r.ready_payload_digest,
        r.icount,
        r.vns,
        r.state_hash,
        r.config_hash,
        r.game_image_hash,
        r.base_image_hash
    ))
}

fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))
}

fn hash_file(path: &Path) -> Result<[u8; 32], String> {
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(*hasher.finalize().as_bytes())
}

fn hash_bytes(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

fn linux_cmdline(paths: &LinuxGuestPaths) -> Result<Vec<u8>, String> {
    let extras = if paths.cmdline_extra.is_empty() {
        "quiet".to_string()
    } else {
        paths.cmdline_extra.join(" ")
    };
    dh_vmm::config::canonicalize_bzimage_cmdline_extras(extras.as_bytes())
        .map_err(|e| format!("BzImage cmdline extras: {e:?}"))
}

const READY_PAYLOAD_LEN: usize = 16;

pub fn parse_ready_payload(payload: &[u8]) -> Result<ReadyPayload, BoundaryError> {
    if payload.len() != READY_PAYLOAD_LEN {
        return Err(BoundaryError::Exit(format!(
            "Ready payload must be {READY_PAYLOAD_LEN} bytes, got {}",
            payload.len()
        )));
    }
    let ready = ReadyPayload {
        unit: u32::from_le_bytes(payload[0..4].try_into().unwrap()),
        region_count: u32::from_le_bytes(payload[4..8].try_into().unwrap()),
        manifest_generation: u64::from_le_bytes(payload[8..16].try_into().unwrap()),
        digest: hash_bytes(payload),
    };
    if ready.region_count == 0 {
        return Err(BoundaryError::Exit(
            "Ready payload region_count must be nonzero".into(),
        ));
    }
    if ready.manifest_generation % 2 != 0 {
        return Err(BoundaryError::Exit(format!(
            "Ready payload manifest_generation {} is odd",
            ready.manifest_generation
        )));
    }
    Ok(ready)
}

fn build_linux_bus(
    config: &MachineConfig,
    base_image: dh_vmm::blkfile::FileBase,
    mem: RuntimeVmMem,
) -> Result<dh_devices::MmioBus, String> {
    let mut bus = dh_devices::MmioBus::new();
    let mut base_image = Some(base_image);
    for id in &config.device_set {
        match *id {
            dh_devices::clock::DEVICE_ID_PV_CLOCK => bus
                .register(
                    dh_devices::clock::PV_CLOCK_BASE,
                    Box::new(dh_devices::clock::PvClock::new(
                        config.clock.num(),
                        config.clock.den(),
                    )),
                )
                .map_err(|e| format!("register pv-clock: {e:?}"))?,
            dh_devices::pad::DEVICE_ID_PV_PAD => bus
                .register(
                    dh_devices::pad::PV_PAD_BASE,
                    Box::new(dh_devices::pad::PvPad::new()),
                )
                .map_err(|e| format!("register pv-pad: {e:?}"))?,
            dh_devices::entropy::DEVICE_ID_PV_ENTROPY => bus
                .register(
                    dh_devices::entropy::PV_ENTROPY_BASE,
                    Box::new(dh_devices::entropy::PvEntropy::new()),
                )
                .map_err(|e| format!("register pv-entropy: {e:?}"))?,
            dh_devices::blk::DEVICE_ID_PV_BLK => {
                let base = base_image
                    .take()
                    .ok_or_else(|| "device_set contains duplicate pv-blk".to_string())?;
                bus.register(
                    PV_BLK_MMIO_BASE,
                    Box::new(dh_devices::blk::PvBlk::new(Box::new(base))),
                )
                .map_err(|e| format!("register pv-blk: {e:?}"))?;
            }
            dh_devices::serial::DEVICE_ID_DEBUG_SERIAL => bus
                .register(
                    DEBUG_SERIAL_MMIO_BASE,
                    Box::new(dh_devices::DebugSerial::new()),
                )
                .map_err(|e| format!("register debug-serial: {e:?}"))?,
            dh_devices::detchannel::DEVICE_ID_DETCHANNEL => bus
                .register(
                    DETCHANNEL_MMIO_BASE,
                    Box::new(CliDetChannel::new(
                        mem.clone(),
                        detguest_host::LogFaultPlan::default(),
                        detguest_host::LogFaultPlan::default,
                    )),
                )
                .map_err(|e| format!("register detchannel: {e:?}"))?,
            other => return Err(format!("unsupported Linux device id {other:#06x}")),
        }
    }
    Ok(bus)
}

fn runtime_detchannel_mut(bus: &mut dh_devices::MmioBus) -> Option<&mut CliDetChannel> {
    bus.devices_mut().find_map(|(_base, dev)| {
        if dev.device_id() != dh_devices::detchannel::DEVICE_ID_DETCHANNEL {
            return None;
        }
        dev.as_any_mut()?.downcast_mut::<CliDetChannel>()
    })
}

fn service_linux_exit(
    rail: &mut dh_vmm::recording::DeviceRail<RuntimeVmMem>,
    icount: u64,
    exit: VcpuExit<'_>,
) -> Result<Vec<detguest_host::GuestEvent>, BoundaryError> {
    let detcall_end = dh_vmm::kvm::PIO_DETCALL_BASE + dh_vmm::kvm::PIO_DETCALL_LEN;
    match exit {
        VcpuExit::IoOut(port, data)
            if (dh_vmm::kvm::PIO_DETCALL_BASE..detcall_end).contains(&port) =>
        {
            let mut ctx = dh_devices::DevCtx::new(
                icount,
                0,
                &mut rail.log,
                &mut rail.mem,
                &mut rail.entropy,
                &mut rail.irqs,
            );
            let host = runtime_detchannel_mut(&mut rail.bus)
                .ok_or_else(|| BoundaryError::Exit("detchannel PIO without device".into()))?;
            let mut word = [0u8; 4];
            let n = data.len().min(4);
            word[..n].copy_from_slice(&data[..n]);
            let events = host
                .host_mut()
                .pio_out(port, u32::from_le_bytes(word), &mut ctx);
            if host.host().metrics.any_anomaly() {
                return Err(BoundaryError::Exit("detchannel drain anomaly".into()));
            }
            if let Some(e) = ctx.log_fault() {
                return Err(BoundaryError::Exit(format!("log fault: {e:?}")));
            }
            Ok(events)
        }
        VcpuExit::IoIn(port, data)
            if (dh_vmm::kvm::PIO_DETCALL_BASE..detcall_end).contains(&port) =>
        {
            let mut ctx = dh_devices::DevCtx::new(
                icount,
                0,
                &mut rail.log,
                &mut rail.mem,
                &mut rail.entropy,
                &mut rail.irqs,
            );
            let host = runtime_detchannel_mut(&mut rail.bus)
                .ok_or_else(|| BoundaryError::Exit("detchannel PIO without device".into()))?;
            let value = host.host_mut().pio_in(port, &mut ctx);
            data.fill(0);
            let bytes = value.to_le_bytes();
            let n = data.len().min(4);
            data[..n].copy_from_slice(&bytes[..n]);
            if host.host().metrics.any_anomaly() {
                return Err(BoundaryError::Exit("detchannel drain anomaly".into()));
            }
            if let Some(e) = ctx.log_fault() {
                return Err(BoundaryError::Exit(format!("log fault: {e:?}")));
            }
            Ok(Vec::new())
        }
        other => {
            rail.service_exit(icount, other)?;
            Ok(Vec::new())
        }
    }
}

fn linux_hash_device_sections(
    bus: &dh_devices::MmioBus,
    lapic: &dh_vmm::lapic::LocalApic,
) -> Vec<u8> {
    let mut bytes = dh_vmm::hash::lapic_section(lapic);
    bytes.extend_from_slice(&dh_vmm::hash::device_sections(bus));
    bytes
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_payload(unit: u32, region_count: u32, manifest_generation: u64) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[0..4].copy_from_slice(&unit.to_le_bytes());
        out[4..8].copy_from_slice(&region_count.to_le_bytes());
        out[8..16].copy_from_slice(&manifest_generation.to_le_bytes());
        out
    }

    #[test]
    fn ready_payload_parser_pins_shape_and_fields() {
        let payload = ready_payload(7, 3, 8);
        let parsed = parse_ready_payload(&payload).expect("valid Ready payload");
        assert_eq!(parsed.unit, 7);
        assert_eq!(parsed.region_count, 3);
        assert_eq!(parsed.manifest_generation, 8);
        assert_eq!(parsed.digest, hash_bytes(&payload));
    }

    #[test]
    fn ready_payload_parser_rejects_malformed_ready() {
        assert!(parse_ready_payload(&ready_payload(7, 0, 8)).is_err());
        assert!(parse_ready_payload(&ready_payload(7, 3, 9)).is_err());
        assert!(parse_ready_payload(&ready_payload(7, 3, 8)[..15]).is_err());
    }
}
