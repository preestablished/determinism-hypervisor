//! Shared rig for the determinism gate tests: boot a guest, run one
//! segment at a time, read the timer-guest ISR table.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use dh_detclock::counter::{InstRetired, NEVER_FIRES_PERIOD};
use dh_vmm::boundary::BoundaryError;
use dh_vmm::config::{BootSpec, CpuidLeaf, MachineConfig};
use dh_vmm::hash::StateHashChain;
use dh_vmm::kvm::{KvmSystem, SlotVm};
use dh_vmm::runctl::{run_segment, Segment, SegmentOutcome, TimerArm, Until};
use kvm_ioctls::VcpuExit;
use vm_memory::Bytes;

#[allow(dead_code)]
pub fn kvm_usable() -> bool {
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

#[allow(dead_code)]
pub const DH_M9_BZIMAGE: &str = "DH_M9_BZIMAGE";
#[allow(dead_code)]
pub const DH_M9_INITRAMFS: &str = "DH_M9_INITRAMFS";
#[allow(dead_code)]
pub const DH_M9_BASE_IMAGE: &str = "DH_M9_BASE_IMAGE";
#[allow(dead_code)]
pub const DH_M9_GAME_IMAGE: &str = "DH_M9_GAME_IMAGE";
#[allow(dead_code)]
pub const DH_M9_IMAGE_CACHE: &str = "DH_M9_IMAGE_CACHE";
#[allow(dead_code)]
pub const DH_M9_ALLOW_SKIP: &str = "DH_M9_ALLOW_SKIP";
#[allow(dead_code)]
pub const M9_LINUX_MEM_BYTES: u64 = 128 * 1024 * 1024;
#[allow(dead_code)]
pub const M9_READY_HARD_CAP: u64 = 10_000_000_000;
#[allow(dead_code)]
pub const M9_DETCHANNEL_MMIO_BASE: u64 = 0xD000_3000;
#[allow(dead_code)]
pub const M9_PV_BLK_MMIO_BASE: u64 = 0xD000_4000;

#[allow(dead_code)]
pub type TestResult<T> = Result<T, String>;

#[allow(dead_code)]
pub const M9_LINUX_ARTIFACT_ENV_VARS: [&str; 5] = [
    DH_M9_BZIMAGE,
    DH_M9_INITRAMFS,
    DH_M9_BASE_IMAGE,
    DH_M9_GAME_IMAGE,
    DH_M9_IMAGE_CACHE,
];

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub struct M9LinuxArtifacts {
    pub bzimage: PathBuf,
    pub initramfs: PathBuf,
    pub base_image: PathBuf,
    pub game_image: PathBuf,
    pub image_cache: PathBuf,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct M9CachedHashes {
    pub bzimage: [u8; 32],
    pub initramfs: [u8; 32],
    pub base_image: [u8; 32],
    pub game_image: [u8; 32],
}

#[allow(dead_code)]
pub const REQUIRED_M9_EXPECTED_REGIONS: &[(&str, i64)] =
    &[("wram", 1), ("framebuffer", 1), ("meta", 1)];

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub struct M9InitramfsContract {
    pub autostart_unit: i64,
    pub exec_path: String,
    pub expected_regions: BTreeMap<String, i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub struct M9GuestEvent {
    pub stream: u32,
    pub icount: u64,
    pub vns: u64,
    pub payload: Vec<u8>,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct M9VmMem(pub vm_memory::GuestMemoryMmap<()>);

impl dh_devices::ctx::GuestMem for M9VmMem {
    fn read(&self, gpa: u64, out: &mut [u8]) -> Result<(), dh_devices::ctx::MemError> {
        self.0
            .read_slice(out, vm_memory::GuestAddress(gpa))
            .map_err(|_| dh_devices::ctx::MemError)
    }

    fn write(&mut self, gpa: u64, data: &[u8]) -> Result<(), dh_devices::ctx::MemError> {
        self.0
            .write_slice(data, vm_memory::GuestAddress(gpa))
            .map_err(|_| dh_devices::ctx::MemError)
    }
}

impl detguest_host::GuestMem for M9VmMem {
    fn read(&self, gpa: u64, out: &mut [u8]) -> Result<(), detguest_host::MemError> {
        self.0
            .read_slice(out, vm_memory::GuestAddress(gpa))
            .map_err(|_| detguest_host::MemError::Unmapped {
                gpa,
                len: out.len(),
            })
    }

    fn write(&mut self, gpa: u64, data: &[u8]) -> Result<(), detguest_host::MemError> {
        self.0
            .write_slice(data, vm_memory::GuestAddress(gpa))
            .map_err(|_| detguest_host::MemError::Unmapped {
                gpa,
                len: data.len(),
            })
    }
}

#[allow(dead_code)]
pub type M9DetChannel = dh_devices::DetChannelDevice<
    M9VmMem,
    detguest_host::LogFaultPlan,
    fn() -> detguest_host::LogFaultPlan,
>;

#[allow(dead_code)]
pub type M9DeviceRail = dh_vmm::recording::DeviceRail<M9VmMem>;

#[allow(dead_code)]
impl M9LinuxArtifacts {
    pub fn from_env_required(test_name: &str) -> Result<Self, String> {
        let artifacts = Self::from_lookup(test_name, |name| std::env::var_os(name))?;
        artifacts.validate_paths()?;
        Ok(artifacts)
    }

    pub fn from_lookup<F>(test_name: &str, mut lookup: F) -> Result<Self, String>
    where
        F: FnMut(&str) -> Option<OsString>,
    {
        let mut missing = Vec::new();
        let mut required = |name: &'static str| -> Option<PathBuf> {
            match lookup(name) {
                Some(value) if !value.is_empty() => Some(PathBuf::from(value)),
                _ => {
                    missing.push(name);
                    None
                }
            }
        };

        let bzimage = required(DH_M9_BZIMAGE);
        let initramfs = required(DH_M9_INITRAMFS);
        let base_image = required(DH_M9_BASE_IMAGE);
        let game_image = required(DH_M9_GAME_IMAGE);
        let image_cache = required(DH_M9_IMAGE_CACHE);

        if !missing.is_empty() {
            return Err(format!(
                "M9 Linux acceptance test {test_name:?} requested, but missing required artifact env vars: {}. Set all of {}. *_ALLOW_SKIP=1 is not accepted for final M9 gates.",
                missing.join(", "),
                M9_LINUX_ARTIFACT_ENV_VARS.join(", ")
            ));
        }

        Ok(Self {
            bzimage: bzimage.expect("missing handled above"),
            initramfs: initramfs.expect("missing handled above"),
            base_image: base_image.expect("missing handled above"),
            game_image: game_image.expect("missing handled above"),
            image_cache: image_cache.expect("missing handled above"),
        })
    }

    fn validate_paths(&self) -> Result<(), String> {
        require_regular_file(DH_M9_BZIMAGE, &self.bzimage)?;
        require_regular_file(DH_M9_INITRAMFS, &self.initramfs)?;
        require_regular_file(DH_M9_BASE_IMAGE, &self.base_image)?;
        require_regular_file(DH_M9_GAME_IMAGE, &self.game_image)?;
        require_directory(DH_M9_IMAGE_CACHE, &self.image_cache)?;
        Ok(())
    }
}

#[allow(dead_code)]
pub fn m9_allow_skip() -> bool {
    std::env::var(DH_M9_ALLOW_SKIP).as_deref() == Ok("1")
}

#[allow(dead_code)]
pub fn m9_artifacts(test_name: &str) -> TestResult<Option<M9LinuxArtifacts>> {
    match M9LinuxArtifacts::from_env_required(test_name) {
        Ok(artifacts) => Ok(Some(artifacts)),
        Err(e) if m9_allow_skip() => {
            eprintln!("skipping M9 Linux acceptance {test_name}: {e}");
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

#[allow(dead_code)]
pub fn m9_kvm_system(test_name: &str) -> TestResult<Option<KvmSystem>> {
    match KvmSystem::open() {
        Ok(sys) => Ok(Some(sys)),
        Err(e) if m9_allow_skip() => {
            eprintln!("skipping M9 Linux acceptance {test_name}: KVM unavailable: {e:?}");
            Ok(None)
        }
        Err(e) => Err(format!("{test_name}: KVM unavailable: {e:?}")),
    }
}

#[allow(dead_code)]
pub fn hash_file(path: &Path) -> TestResult<[u8; 32]> {
    use std::io::Read;

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

#[allow(dead_code)]
pub fn hash_bytes(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

#[allow(dead_code)]
pub fn parse_m9_boot_toml_contract(boot_toml: &str) -> TestResult<M9InitramfsContract> {
    let manifest: toml::Value = boot_toml
        .parse()
        .map_err(|e| format!("parse initramfs boot.toml: {e}"))?;
    let root = toml_table(&manifest, "boot.toml root")?;
    if root
        .get("boot_toml_version")
        .and_then(|value| value.as_integer())
        != Some(1)
    {
        return Err("boot.toml must set boot_toml_version = 1".into());
    }

    let autostart_unit = root
        .get("autostart")
        .and_then(|value| value.as_table())
        .and_then(|table| table.get("unit"))
        .and_then(|value| value.as_integer())
        .ok_or_else(|| "boot.toml must autostart the reference workload unit".to_string())?;
    let units = toml_array(root, "unit")?;
    let unit = units
        .iter()
        .filter_map(|value| value.as_table())
        .find(|table| table.get("id").and_then(|value| value.as_integer()) == Some(autostart_unit))
        .ok_or_else(|| format!("boot.toml autostart unit {autostart_unit} has no [[unit]]"))?;
    let exec_path = unit
        .get("exec")
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("boot.toml autostart unit {autostart_unit} must set exec"))?;
    if matches!(exec_path, "/opt/autostart-trivial" | "/opt/print-lines") {
        return Err(format!(
            "initramfs boot.toml is the M2 smoke manifest, not the M9 reference-workload contract: autostart exec={exec_path:?}"
        ));
    }

    let control = unit
        .get("control")
        .and_then(|value| value.as_table())
        .ok_or_else(|| "autostart unit must declare [unit.control]".to_string())?;
    if control.get("protocol").and_then(|value| value.as_str()) != Some("refwork-ctl") {
        return Err("autostart unit.control must use protocol = \"refwork-ctl\"".into());
    }
    if control
        .get("proto_version")
        .and_then(|value| value.as_integer())
        != Some(1)
    {
        return Err("autostart unit.control must use proto_version = 1".into());
    }
    if control.get("game_dev").and_then(|value| value.as_str()) != Some("/dev/vdb") {
        return Err("autostart unit.control must set game_dev = \"/dev/vdb\"".into());
    }

    let expected_region_tables = toml_array(root, "expected_region")?;
    if expected_region_tables.is_empty() {
        return Err("boot.toml must list expected regions for the Ready gate".into());
    }
    let mut expected_regions = BTreeMap::new();
    for (idx, region) in expected_region_tables.iter().enumerate() {
        let region = region
            .as_table()
            .ok_or_else(|| format!("expected_region[{idx}] must be a table"))?;
        let name = region
            .get("name")
            .and_then(|value| value.as_str())
            .ok_or_else(|| format!("expected_region[{idx}] must name a region"))?;
        if name.is_empty() {
            return Err(format!("expected_region[{idx}] has an empty name"));
        }
        let layout_version = region
            .get("layout_version")
            .and_then(|value| value.as_integer())
            .filter(|version| *version > 0)
            .ok_or_else(|| {
                format!("expected_region[{idx}] ({name}) must pin a positive layout_version")
            })?;
        if expected_regions
            .insert(name.to_string(), layout_version)
            .is_some()
        {
            return Err(format!("duplicate expected_region name {name:?}"));
        }
    }

    for (name, layout_version) in REQUIRED_M9_EXPECTED_REGIONS {
        if expected_regions.get(*name) != Some(layout_version) {
            return Err(format!(
                "boot.toml must list expected_region {name:?} with layout_version {layout_version}"
            ));
        }
    }

    Ok(M9InitramfsContract {
        autostart_unit,
        exec_path: exec_path.to_string(),
        expected_regions,
    })
}

#[allow(dead_code)]
pub fn assert_m9_initramfs_contract(initramfs: &Path) -> TestResult<M9InitramfsContract> {
    let archive = std::fs::read(initramfs)
        .map_err(|e| format!("read initramfs {}: {e}", initramfs.display()))?;
    let boot_toml = initramfs_entry(&archive, "etc/detguest/boot.toml")?;
    let boot_toml = std::str::from_utf8(boot_toml.data)
        .map_err(|e| format!("boot.toml in {} is not UTF-8: {e}", initramfs.display()))?;
    let contract = parse_m9_boot_toml_contract(boot_toml)?;

    initramfs_entry(&archive, "init")?;
    let agent = initramfs_entry(&archive, "sbin/detguest-agent")?;
    if !agent.is_executable() {
        return Err("initramfs sbin/detguest-agent must be executable".into());
    }

    let exec_entry = initramfs_entry(&archive, &contract.exec_path)?;
    if !exec_entry.is_executable() {
        return Err(format!(
            "initramfs autostart exec {} must be executable",
            contract.exec_path
        ));
    }

    Ok(contract)
}

fn toml_table<'a>(
    value: &'a toml::Value,
    name: &str,
) -> TestResult<&'a toml::map::Map<String, toml::Value>> {
    value
        .as_table()
        .ok_or_else(|| format!("{name} must be a TOML table"))
}

fn toml_array<'a>(table: &'a toml::Table, key: &str) -> TestResult<&'a Vec<toml::Value>> {
    table
        .get(key)
        .and_then(|value| value.as_array())
        .ok_or_else(|| format!("boot.toml missing array [[{key}]]"))
}

struct InitramfsEntry<'a> {
    mode: u32,
    data: &'a [u8],
}

impl InitramfsEntry<'_> {
    fn is_executable(&self) -> bool {
        let kind = self.mode & 0o170000;
        matches!(kind, 0o100000 | 0o120000) && self.mode & 0o111 != 0
    }
}

fn initramfs_entry<'a>(archive: &'a [u8], needle: &str) -> TestResult<InitramfsEntry<'a>> {
    const HEADER_LEN: usize = 110;
    let needle = normalize_cpio_path(needle);
    let mut offset = 0usize;
    while offset
        .checked_add(HEADER_LEN)
        .is_some_and(|end| end <= archive.len())
    {
        let header = &archive[offset..offset + HEADER_LEN];
        if &header[..6] != b"070701" {
            return Err(format!(
                "unsupported initramfs cpio magic at offset {offset}"
            ));
        }
        let mode = parse_newc_hex(&header[14..22])? as u32;
        let file_size = parse_newc_hex(&header[54..62])?;
        let name_size = parse_newc_hex(&header[94..102])?;
        offset += HEADER_LEN;

        let name_end = offset
            .checked_add(name_size)
            .ok_or_else(|| "newc filename offset overflow".to_string())?;
        if name_size == 0 || name_end > archive.len() {
            return Err("truncated initramfs cpio filename".into());
        }
        let raw_name = &archive[offset..name_end - 1];
        let name = std::str::from_utf8(raw_name).map_err(|e| format!("newc filename utf8: {e}"))?;
        let data_start = align4(name_end).ok_or_else(|| "newc data offset overflow".to_string())?;
        let data_end = data_start
            .checked_add(file_size)
            .ok_or_else(|| "newc file size overflow".to_string())?;
        if data_end > archive.len() {
            return Err(format!("truncated initramfs cpio entry {name:?}"));
        }
        if name == "TRAILER!!!" {
            break;
        }
        if normalize_cpio_path(name) == needle {
            return Ok(InitramfsEntry {
                mode,
                data: &archive[data_start..data_end],
            });
        }
        offset = align4(data_end).ok_or_else(|| "newc next offset overflow".to_string())?;
    }
    Err(format!("initramfs missing {needle}"))
}

fn normalize_cpio_path(path: &str) -> &str {
    path.trim_start_matches("./").trim_start_matches('/')
}

fn align4(n: usize) -> Option<usize> {
    n.checked_add(3).map(|v| v & !3)
}

fn parse_newc_hex(field: &[u8]) -> TestResult<usize> {
    let text = std::str::from_utf8(field).map_err(|e| format!("newc header utf8: {e}"))?;
    usize::from_str_radix(text, 16).map_err(|e| format!("newc header hex {text:?}: {e}"))
}

#[allow(dead_code)]
pub fn m9_cache_entry(cache_root: &Path, hash: &[u8; 32]) -> PathBuf {
    cache_root.join(hex(hash))
}

#[allow(dead_code)]
pub fn ensure_cache_entry(source: &Path, cache_root: &Path) -> TestResult<[u8; 32]> {
    let hash = hash_file(source)?;
    let dest = m9_cache_entry(cache_root, &hash);
    if dest.exists() {
        if hash_file(&dest)? == hash {
            return Ok(hash);
        }
        return Err(format!(
            "existing image cache entry {} does not match its content-addressed key",
            dest.display()
        ));
    }

    match std::fs::hard_link(source, &dest) {
        Ok(()) => Ok(hash),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            if hash_file(&dest)? == hash {
                Ok(hash)
            } else {
                Err(format!(
                    "concurrent image cache entry {} does not match its content-addressed key",
                    dest.display()
                ))
            }
        }
        Err(_) => {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default();
            let tmp = cache_root.join(format!(
                "{}.{}.{}.tmp",
                dest.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("m9-cache"),
                std::process::id(),
                nonce
            ));
            let _ = std::fs::remove_file(&tmp);
            std::fs::copy(source, &tmp).map_err(|e| {
                format!(
                    "copy {} to temporary image cache entry {}: {e}",
                    source.display(),
                    tmp.display()
                )
            })?;
            if hash_file(&tmp)? != hash {
                let _ = std::fs::remove_file(&tmp);
                return Err(format!(
                    "temporary image cache entry {} hash mismatch",
                    tmp.display()
                ));
            }
            let publish = match std::fs::hard_link(&tmp, &dest) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    if hash_file(&dest)? == hash {
                        Ok(())
                    } else {
                        Err(format!(
                            "concurrent image cache entry {} does not match its content-addressed key",
                            dest.display()
                        ))
                    }
                }
                Err(e) => Err(format!(
                    "publish temporary image cache entry {} to {}: {e}",
                    tmp.display(),
                    dest.display()
                )),
            };
            let cleanup = std::fs::remove_file(&tmp)
                .map_err(|e| format!("remove temporary image cache entry {}: {e}", tmp.display()));
            publish?;
            cleanup?;
            Ok(hash)
        }
    }
}

#[allow(dead_code)]
pub fn populate_m9_image_cache(artifacts: &M9LinuxArtifacts) -> TestResult<M9CachedHashes> {
    Ok(M9CachedHashes {
        bzimage: ensure_cache_entry(&artifacts.bzimage, &artifacts.image_cache)?,
        initramfs: ensure_cache_entry(&artifacts.initramfs, &artifacts.image_cache)?,
        base_image: ensure_cache_entry(&artifacts.base_image, &artifacts.image_cache)?,
        game_image: ensure_cache_entry(&artifacts.game_image, &artifacts.image_cache)?,
    })
}

#[allow(dead_code)]
pub fn m9_linux_machine_config(
    hashes: &M9CachedHashes,
    cpuid_table: Vec<CpuidLeaf>,
) -> MachineConfig {
    let mut config = MachineConfig::new(
        M9_LINUX_MEM_BYTES,
        hashes.game_image,
        BootSpec::BzImage {
            kernel_hash: hashes.bzimage,
            initramfs_hash: hashes.initramfs,
            cmdline: dh_vmm::config::canonicalize_bzimage_cmdline_extras(b"quiet")
                .expect("M9 allows quiet as an append-only cmdline extra"),
        },
    );
    config.cpuid_table = cpuid_table;
    config.device_set = vec![
        dh_devices::detchannel::DEVICE_ID_DETCHANNEL,
        dh_devices::clock::DEVICE_ID_PV_CLOCK,
        dh_devices::pad::DEVICE_ID_PV_PAD,
        dh_devices::entropy::DEVICE_ID_PV_ENTROPY,
        dh_devices::blk::DEVICE_ID_PV_BLK,
        dh_devices::serial::DEVICE_ID_DEBUG_SERIAL,
    ];
    config
}

#[allow(dead_code)]
pub fn m9_linux_bus(
    config: &MachineConfig,
    base_image: dh_vmm::blkfile::FileBase,
    mem: M9VmMem,
) -> TestResult<dh_devices::MmioBus> {
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
                    M9_PV_BLK_MMIO_BASE,
                    Box::new(dh_devices::blk::PvBlk::new(Box::new(base))),
                )
                .map_err(|e| format!("register pv-blk: {e:?}"))?;
            }
            dh_devices::serial::DEVICE_ID_DEBUG_SERIAL => bus
                .register(
                    M9_PV_BLK_MMIO_BASE + 0x2000,
                    Box::new(dh_devices::DebugSerial::new()),
                )
                .map_err(|e| format!("register debug-serial: {e:?}"))?,
            dh_devices::detchannel::DEVICE_ID_DETCHANNEL => bus
                .register(
                    M9_DETCHANNEL_MMIO_BASE,
                    Box::new(dh_devices::DetChannelDevice::new(
                        mem.clone(),
                        detguest_host::LogFaultPlan::default(),
                        fresh_log_fault_plan as fn() -> detguest_host::LogFaultPlan,
                    )),
                )
                .map_err(|e| format!("register detchannel: {e:?}"))?,
            other => return Err(format!("unsupported M9 device id {other:#06x}")),
        }
    }
    Ok(bus)
}

#[allow(dead_code)]
pub fn fresh_log_fault_plan() -> detguest_host::LogFaultPlan {
    detguest_host::LogFaultPlan::default()
}

#[allow(dead_code)]
pub fn m9_detchannel_mut(bus: &mut dh_devices::MmioBus) -> Option<&mut M9DetChannel> {
    bus.devices_mut().find_map(|(_base, dev)| {
        if dev.device_id() != dh_devices::detchannel::DEVICE_ID_DETCHANNEL {
            return None;
        }
        dev.as_any_mut()?.downcast_mut::<M9DetChannel>()
    })
}

#[allow(dead_code)]
pub fn m9_service_exit_with_detchannel(
    rail: &mut M9DeviceRail,
    icount: u64,
    exit: VcpuExit<'_>,
) -> Result<Vec<M9GuestEvent>, BoundaryError> {
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
            let host = m9_detchannel_mut(&mut rail.bus).ok_or_else(|| {
                BoundaryError::Exit("detchannel PIO without DetChannelDevice".into())
            })?;
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
            drained_guest_events(events, icount)
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
            let host = m9_detchannel_mut(&mut rail.bus).ok_or_else(|| {
                BoundaryError::Exit("detchannel PIO without DetChannelDevice".into())
            })?;
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

#[allow(dead_code)]
fn drained_guest_events(
    events: Vec<detguest_host::GuestEvent>,
    icount: u64,
) -> Result<Vec<M9GuestEvent>, BoundaryError> {
    events
        .into_iter()
        .map(|ev| {
            let (stream, payload) = dh_devices::detchannel::stream_guest_event_payload(&ev)
                .ok_or_else(|| {
                    BoundaryError::Exit(
                        "detchannel guest event could not be encoded for streaming".into(),
                    )
                })?;
            Ok(M9GuestEvent {
                stream: u32::from(stream),
                icount,
                vns: ev.vnanos,
                payload,
            })
        })
        .collect()
}

#[allow(dead_code)]
pub fn m9_runtime_hash_device_sections(rail: &RefCell<M9DeviceRail>) -> Vec<u8> {
    let rail = rail.borrow();
    let mut bytes = dh_vmm::hash::lapic_section(&rail.lapic);
    bytes.extend_from_slice(&dh_vmm::hash::device_sections(&rail.bus));
    bytes
}

#[allow(dead_code)]
fn require_regular_file(env_name: &str, path: &Path) -> Result<(), String> {
    let meta = std::fs::metadata(path)
        .map_err(|e| format!("{env_name}={} is not readable: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!(
            "{env_name}={} must name a regular file",
            path.display()
        ));
    }
    Ok(())
}

#[allow(dead_code)]
fn require_directory(env_name: &str, path: &Path) -> Result<(), String> {
    let meta = std::fs::metadata(path)
        .map_err(|e| format!("{env_name}={} is not readable: {e}", path.display()))?;
    if !meta.is_dir() {
        return Err(format!(
            "{env_name}={} must name an existing directory",
            path.display()
        ));
    }
    Ok(())
}

#[allow(dead_code)] // used by if0_deferral, not timer_determinism (per-test compilation)
pub fn hex(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[allow(dead_code)]
pub struct Rig {
    pub slot: SlotVm,
    pub counter: InstRetired,
    pub chain: StateHashChain,
    pub config: MachineConfig,
}

#[allow(dead_code)]
impl Rig {
    pub fn boot(elf: &[u8], cmdline: &[u8]) -> Result<Rig, String> {
        dh_vmm::run::install_kick_handler().map_err(|e| format!("kick: {e}"))?;
        let sys = KvmSystem::open().map_err(|e| format!("{e:?}"))?;
        let slot = sys.create_slot_vm(16 << 20).map_err(|e| format!("{e:?}"))?;
        dh_vmm::boot::load_and_enter(&slot, elf, cmdline).map_err(|e| format!("{e}"))?;
        let counter = InstRetired::open_for_current_thread().map_err(|e| format!("{e:?}"))?;
        counter
            .route_overflow_to_thread(dh_vmm::run::current_tid(), dh_vmm::run::kick_signal())
            .map_err(|e| format!("{e:?}"))?;
        counter
            .arm_period(NEVER_FIRES_PERIOD)
            .map_err(|e| format!("{e:?}"))?;
        counter.reset().map_err(|e| format!("{e:?}"))?;
        counter.enable().map_err(|e| format!("{e:?}"))?;
        let config = MachineConfig::new(
            16 << 20,
            [3; 32],
            BootSpec::Elf {
                kernel_hash: [3; 32],
                cmdline: cmdline.to_vec(),
            },
        );
        Ok(Rig {
            slot,
            counter,
            chain: StateHashChain::new(&[3; 32], &[3; 32]),
            config,
        })
    }

    /// One segment from the current position to `budget` (absolute).
    pub fn run_one(
        &mut self,
        timer: Option<TimerArm>,
        budget_abs: u64,
    ) -> Result<SegmentOutcome, String> {
        let start = self.counter.read().map_err(|e| format!("{e:?}"))?;
        let pause = AtomicBool::new(false);
        let mut seg = Segment {
            slot: &mut self.slot,
            counter: &self.counter,
            chain: &mut self.chain,
            config: &self.config,
            start_icount: start,
            injections: &[],
            timer,
            pause: &pause,
            sdk_events: None,
            hash_device_sections: None,
        };
        run_segment(
            &mut seg,
            Until::IcountBudget(budget_abs.saturating_sub(start)),
            &mut || false,
            &mut |exit: VcpuExit| Err(BoundaryError::Exit(format!("unexpected exit: {exit:?}"))),
        )
        .map_err(|e| format!("{e}"))
    }

    pub fn read_table(&self) -> (u64, Vec<u8>) {
        let mut head = [0u8; 8];
        self.slot
            .guest_mem
            .read_slice(
                &mut head,
                vm_memory::GuestAddress(nanokernel::TIMER_GUEST_TABLE_GPA),
            )
            .unwrap();
        let count = u64::from_le_bytes(head);
        let mut vecs = vec![0u8; count as usize];
        self.slot
            .guest_mem
            .read_slice(
                &mut vecs,
                vm_memory::GuestAddress(nanokernel::TIMER_GUEST_TABLE_GPA + 8),
            )
            .unwrap();
        (count, vecs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m9_artifact_lookup_reports_all_missing_vars() {
        let err = M9LinuxArtifacts::from_lookup("linux-ready", |_| None).unwrap_err();
        for name in M9_LINUX_ARTIFACT_ENV_VARS {
            assert!(err.contains(name), "error did not mention {name}: {err}");
        }
        assert!(err.contains("*_ALLOW_SKIP=1"));
    }

    #[test]
    fn m9_artifact_lookup_accepts_all_required_vars() {
        let artifacts = M9LinuxArtifacts::from_lookup("linux-ready", |name| {
            Some(OsString::from(format!("/tmp/{name}")))
        })
        .unwrap();
        assert_eq!(artifacts.bzimage, PathBuf::from("/tmp/DH_M9_BZIMAGE"));
        assert_eq!(
            artifacts.image_cache,
            PathBuf::from("/tmp/DH_M9_IMAGE_CACHE")
        );
    }

    #[test]
    fn m9_boot_contract_parser_accepts_reference_manifest() {
        let boot_toml = r#"
boot_toml_version = 1

[autostart]
unit = 0

[[unit]]
id = 0
exec = "/opt/m9-refwork-contract"

[unit.control]
protocol = "refwork-ctl"
proto_version = 1
game_dev = "/dev/vdb"

[[expected_region]]
name = "wram"
layout_version = 1

[[expected_region]]
name = "framebuffer"
layout_version = 1

[[expected_region]]
name = "meta"
layout_version = 1
"#;

        let contract = parse_m9_boot_toml_contract(boot_toml).unwrap();
        assert_eq!(contract.autostart_unit, 0);
        assert_eq!(contract.exec_path, "/opt/m9-refwork-contract");
        assert_eq!(contract.expected_regions["wram"], 1);
        assert_eq!(contract.expected_regions["framebuffer"], 1);
        assert_eq!(contract.expected_regions["meta"], 1);
    }

    #[test]
    fn m9_boot_contract_parser_rejects_smoke_manifest() {
        let boot_toml = r#"
boot_toml_version = 1

[autostart]
unit = 0

[[unit]]
id = 0
exec = "/opt/autostart-trivial"
"#;

        let err = parse_m9_boot_toml_contract(boot_toml).unwrap_err();
        assert!(err.contains("M2 smoke manifest"), "unexpected error: {err}");
    }
}
