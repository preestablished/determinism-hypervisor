//! MachineConfig + canonical encoding (ARCH §2.2 / §8.1 MCFG, API.md §2.1).
//!
//! The canonical byte encoding defined here is the `machine_config_hash`
//! preimage — it feeds H_0 of the state-hash chain (§8.5) and is embedded in
//! every snapshot's MCFG section, so it must be byte-stable forever; extend
//! it only by bumping `version`.
//!
//! `entropy_seed` is deliberately NOT a field: it is a per-segment parameter
//! (CreateVm / DHILOG header, ARCH §5). The determinism class of a machine
//! must not vary per seed.

use crate::vt::ClockRatio;

/// One masked CPUID leaf (function, index → registers). The table is the
/// §2.2 CPUID determinism mask's output slot; the mask itself lands with the
/// CPUID bead. Must be sorted by (function, index), no duplicates.
///
/// `flags` carries the KVM entry flags (SIGNIFCANT_INDEX etc.) — they
/// shape how KVM dispatches subleaves, so they are machine behavior and
/// part of the ONE canonical preimage (bead nq5: cpuid.rs's table hash
/// and this struct's encoding must never fork on representation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuidLeaf {
    pub function: u32,
    pub index: u32,
    pub flags: u32,
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
}

impl CpuidLeaf {
    /// THE canonical leaf encoding — the single preimage shared by
    /// MachineConfig's encoding and cpuid.rs's `cpuid_table_hash`.
    pub fn encode_into(&self, out: &mut Vec<u8>) {
        for v in [
            self.function,
            self.index,
            self.flags,
            self.eax,
            self.ebx,
            self.ecx,
            self.edx,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
}

/// Canonical hash of a sorted leaf table (the MachineConfig determinism
/// class's CPUID component): blake3 over [`CpuidLeaf::encode_into`].
pub fn cpuid_leaves_hash(leaves: &[CpuidLeaf]) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(leaves.len() * 28);
    for leaf in leaves {
        leaf.encode_into(&mut bytes);
    }
    *blake3::hash(&bytes).as_bytes()
}

/// Boot selection (API.md §2.1 BootSpec). Phase 1 boots ELF; BzImage fields
/// are carried (and encoded) but the loader lands later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootSpec {
    Elf {
        kernel_hash: [u8; 32],
        cmdline: Vec<u8>,
    },
    BzImage {
        kernel_hash: [u8; 32],
        initramfs_hash: [u8; 32],
        cmdline: Vec<u8>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashEpochs {
    EpochsOn,
    FinalOnly,
}

pub const CONFIG_VERSION: u32 = 1;
pub const MEM_ALIGN: u64 = 2 * 1024 * 1024;
pub const DEFAULT_EPOCH_LEN: u64 = 50_000_000;
pub const DEFAULT_SKID_MARGIN: u32 = 8192;
pub const DEFAULT_RESYNC_SLACK: u32 = 1024;
/// Bounds on variable-length parts — keeps every length far from the u32
/// cast and gives the forever-frozen format hard decode limits.
pub const MAX_CMDLINE: usize = 4096;
pub const MAX_CPUID_LEAVES: usize = 4096;
pub const MAX_DEVICES: usize = 256;
pub const BZIMAGE_FORCED_CMDLINE: &[u8] = b"console=ttyS0 nokaslr norandmaps random.trust_cpu=off tsc=unstable clocksource=dh-pvclock nohz=off highres=off init=/init";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BzImageCmdlineExtras {
    quiet: bool,
    loglevel: Option<u8>,
}

pub fn canonicalize_bzimage_cmdline_extras(extras: &[u8]) -> Result<Vec<u8>, ConfigError> {
    let extras = parse_bzimage_cmdline_extras(extras)?;
    compose_bzimage_cmdline(BZIMAGE_FORCED_CMDLINE, extras)
}

pub fn bzimage_cmdline_extras_from_canonical(cmdline: &[u8]) -> Result<Vec<u8>, ConfigError> {
    if cmdline == BZIMAGE_FORCED_CMDLINE {
        return Ok(Vec::new());
    }
    let Some(rest) = cmdline.strip_prefix(BZIMAGE_FORCED_CMDLINE) else {
        return Err(ConfigError::BzImageCmdlineNotCanonical);
    };
    let Some(extras) = rest.strip_prefix(b" ") else {
        return Err(ConfigError::BzImageCmdlineNotCanonical);
    };
    let recanonicalized = canonicalize_bzimage_cmdline_extras(extras)?;
    if recanonicalized != cmdline {
        return Err(ConfigError::BzImageCmdlineNotCanonical);
    }
    Ok(extras.to_vec())
}

fn parse_bzimage_cmdline_extras(extras: &[u8]) -> Result<BzImageCmdlineExtras, ConfigError> {
    if extras.is_empty() {
        return Ok(BzImageCmdlineExtras::default());
    }
    if extras.contains(&0) {
        return Err(ConfigError::BzImageCmdlineNul);
    }
    if !extras.is_ascii() {
        return Err(ConfigError::BzImageCmdlineNonAscii);
    }
    if extras.first().is_some_and(u8::is_ascii_whitespace)
        || extras.last().is_some_and(u8::is_ascii_whitespace)
        || extras
            .windows(2)
            .any(|w| w[0].is_ascii_whitespace() && w[1].is_ascii_whitespace())
    {
        return Err(ConfigError::BzImageCmdlineEmptyToken);
    }

    let mut parsed = BzImageCmdlineExtras::default();
    for token in extras.split(u8::is_ascii_whitespace) {
        if is_forced_bzimage_baseline_key(token) {
            return Err(ConfigError::BzImageCmdlineBaselineOverride);
        }
        if token == b"quiet" {
            if parsed.quiet {
                return Err(ConfigError::BzImageCmdlineDuplicateToken);
            }
            parsed.quiet = true;
            continue;
        }
        if let Some(raw) = token.strip_prefix(b"loglevel=") {
            if parsed.loglevel.is_some() {
                return Err(ConfigError::BzImageCmdlineDuplicateToken);
            }
            if raw.len() != 1 || !(b'0'..=b'7').contains(&raw[0]) {
                return Err(ConfigError::BzImageCmdlineUnsupportedToken);
            }
            parsed.loglevel = Some(raw[0] - b'0');
            continue;
        }
        return Err(ConfigError::BzImageCmdlineUnsupportedToken);
    }
    Ok(parsed)
}

fn compose_bzimage_cmdline(
    baseline: &[u8],
    extras: BzImageCmdlineExtras,
) -> Result<Vec<u8>, ConfigError> {
    let mut out = baseline.to_vec();
    if extras.quiet {
        out.extend_from_slice(b" quiet");
    }
    if let Some(level) = extras.loglevel {
        out.extend_from_slice(b" loglevel=");
        out.push(b'0' + level);
    }
    if out.len() > MAX_CMDLINE {
        return Err(ConfigError::CmdlineTooLong);
    }
    Ok(out)
}

fn validate_canonical_bzimage_cmdline(cmdline: &[u8]) -> Result<(), ConfigError> {
    if cmdline.len() > MAX_CMDLINE {
        return Err(ConfigError::CmdlineTooLong);
    }
    bzimage_cmdline_extras_from_canonical(cmdline).map(|_| ())
}

fn is_forced_bzimage_baseline_key(token: &[u8]) -> bool {
    let key = token
        .split(|b| *b == b'=')
        .next()
        .expect("split always yields one item");
    matches!(
        key,
        b"console"
            | b"nokaslr"
            | b"norandmaps"
            | b"random.trust_cpu"
            | b"tsc"
            | b"clocksource"
            | b"nohz"
            | b"highres"
            | b"init"
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineConfig {
    /// Config schema version (currently 1).
    pub version: u32,
    /// Guest memory, multiple of 2 MiB.
    pub mem_bytes: u64,
    /// MUST be 1 in v1.
    pub vcpus: u32,
    /// Virtual ns per instruction (fixed for the VM lifetime, ARCH §4).
    pub clock: ClockRatio,
    /// BLAKE3 of the base disk image (image cache key).
    pub base_image_hash: [u8; 32],
    pub boot: BootSpec,
    /// Instructions per epoch (hash grid).
    pub epoch_len: u64,
    pub hash_epochs: HashEpochs,
    /// PMI landing margin in instructions. Landing-only: ARCH §3.2
    /// guarantees the landed result is independent of it, so it is carried
    /// as config but EXCLUDED from the canonical encoding / preimage —
    /// changing a landing knob must not fork snapshot identity.
    pub skid_margin: u32,
    /// Single-step refinement slack (Phase-1 landing knob). Excluded from
    /// the preimage for the same reason; also not a wire field.
    pub resync_slack: u32,
    /// Masked CPUID table; sorted by (function, index), unique.
    pub cpuid_table: Vec<CpuidLeaf>,
    /// Device ids present on the MMIO bus; sorted, unique.
    pub device_set: Vec<u16>,
}

impl MachineConfig {
    /// Phase-1 defaults around a caller-supplied boot + memory size.
    pub fn new(mem_bytes: u64, base_image_hash: [u8; 32], boot: BootSpec) -> Self {
        Self {
            version: CONFIG_VERSION,
            mem_bytes,
            vcpus: 1,
            clock: ClockRatio::default(),
            base_image_hash,
            boot,
            epoch_len: DEFAULT_EPOCH_LEN,
            hash_epochs: HashEpochs::EpochsOn,
            skid_margin: DEFAULT_SKID_MARGIN,
            resync_slack: DEFAULT_RESYNC_SLACK,
            cpuid_table: Vec::new(),
            device_set: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.version != CONFIG_VERSION {
            return Err(ConfigError::UnsupportedVersion);
        }
        if self.mem_bytes == 0 || !self.mem_bytes.is_multiple_of(MEM_ALIGN) {
            return Err(ConfigError::MemNotAligned);
        }
        if self.vcpus != 1 {
            return Err(ConfigError::VcpusNotOne);
        }
        if self.epoch_len == 0 {
            return Err(ConfigError::ZeroEpochLen);
        }
        let cmdline = match &self.boot {
            BootSpec::Elf { cmdline, .. } | BootSpec::BzImage { cmdline, .. } => cmdline,
        };
        if cmdline.len() > MAX_CMDLINE {
            return Err(ConfigError::CmdlineTooLong);
        }
        if let BootSpec::BzImage { cmdline, .. } = &self.boot {
            validate_canonical_bzimage_cmdline(cmdline)?;
        }
        if self.cpuid_table.len() > MAX_CPUID_LEAVES {
            return Err(ConfigError::CpuidTableTooLarge);
        }
        if self.device_set.len() > MAX_DEVICES {
            return Err(ConfigError::DeviceSetTooLarge);
        }
        if !self
            .cpuid_table
            .windows(2)
            .all(|w| (w[0].function, w[0].index) < (w[1].function, w[1].index))
        {
            return Err(ConfigError::CpuidTableUnsorted);
        }
        if !self.device_set.windows(2).all(|w| w[0] < w[1]) {
            return Err(ConfigError::DeviceSetUnsorted);
        }
        Ok(())
    }

    /// Canonical encoding — the `machine_config_hash` preimage. Fixed field
    /// order, little-endian, length-prefixed variable parts; no maps, no
    /// floats, no platform-dependent widths. Validates first: an invalid
    /// config has no canonical form.
    pub fn canonical_encode(&self) -> Result<Vec<u8>, ConfigError> {
        self.validate()?;
        let mut out = Vec::with_capacity(256);
        out.extend_from_slice(b"DHMCFG\0\0"); // 8-byte domain separator
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&self.mem_bytes.to_le_bytes());
        out.extend_from_slice(&self.vcpus.to_le_bytes());
        out.extend_from_slice(&self.clock.num().to_le_bytes());
        out.extend_from_slice(&self.clock.den().to_le_bytes());
        out.extend_from_slice(&self.base_image_hash);
        match &self.boot {
            BootSpec::Elf {
                kernel_hash,
                cmdline,
            } => {
                out.push(1);
                out.extend_from_slice(kernel_hash);
                out.extend_from_slice(&(cmdline.len() as u32).to_le_bytes());
                out.extend_from_slice(cmdline);
            }
            BootSpec::BzImage {
                kernel_hash,
                initramfs_hash,
                cmdline,
            } => {
                out.push(2);
                out.extend_from_slice(kernel_hash);
                out.extend_from_slice(initramfs_hash);
                out.extend_from_slice(&(cmdline.len() as u32).to_le_bytes());
                out.extend_from_slice(cmdline);
            }
        }
        // epoch_len/hash_epochs ARE preimage members: epoch hashes feed the
        // §8.5 chain, so they change the comparison object itself. The
        // landing knobs (skid_margin, resync_slack) are deliberately NOT
        // encoded — see the field docs.
        out.extend_from_slice(&self.epoch_len.to_le_bytes());
        out.push(match self.hash_epochs {
            HashEpochs::EpochsOn => 1,
            HashEpochs::FinalOnly => 2,
        });
        out.extend_from_slice(&(self.cpuid_table.len() as u32).to_le_bytes());
        for leaf in &self.cpuid_table {
            leaf.encode_into(&mut out);
        }
        out.extend_from_slice(&(self.device_set.len() as u32).to_le_bytes());
        for id in &self.device_set {
            out.extend_from_slice(&id.to_le_bytes());
        }
        Ok(out)
    }

    /// Decode the frozen v1 canonical representation written into DHSNAP
    /// `MCFG` sections. Landing-only knobs are not in the preimage, so they
    /// recover to their defaults while preserving the exact canonical bytes.
    pub fn canonical_decode(bytes: &[u8]) -> Result<Self, ConfigDecodeError> {
        let mut r = ConfigReader::new(bytes);
        if r.read_exact(8)? != b"DHMCFG\0\0" {
            return Err(ConfigDecodeError::BadMagic);
        }
        let version = r.read_u32()?;
        let mem_bytes = r.read_u64()?;
        let vcpus = r.read_u32()?;
        let clock_num = r.read_u32()?;
        let clock_den = r.read_u32()?;
        let clock =
            ClockRatio::new(clock_num, clock_den).ok_or(ConfigDecodeError::ZeroClockTerm)?;
        let base_image_hash = r.read_hash()?;
        let boot = match r.read_u8()? {
            1 => BootSpec::Elf {
                kernel_hash: r.read_hash()?,
                cmdline: r.read_vec_u32("cmdline", MAX_CMDLINE)?,
            },
            2 => BootSpec::BzImage {
                kernel_hash: r.read_hash()?,
                initramfs_hash: r.read_hash()?,
                cmdline: r.read_vec_u32("cmdline", MAX_CMDLINE)?,
            },
            found => return Err(ConfigDecodeError::BadBootTag { found }),
        };
        let epoch_len = r.read_u64()?;
        let hash_epochs = match r.read_u8()? {
            1 => HashEpochs::EpochsOn,
            2 => HashEpochs::FinalOnly,
            found => return Err(ConfigDecodeError::BadHashEpochs { found }),
        };
        let cpuid_count = r.read_u32()? as usize;
        if cpuid_count > MAX_CPUID_LEAVES {
            return Err(ConfigDecodeError::InvalidConfig(
                ConfigError::CpuidTableTooLarge,
            ));
        }
        let mut cpuid_table = Vec::with_capacity(cpuid_count);
        for _ in 0..cpuid_count {
            cpuid_table.push(CpuidLeaf {
                function: r.read_u32()?,
                index: r.read_u32()?,
                flags: r.read_u32()?,
                eax: r.read_u32()?,
                ebx: r.read_u32()?,
                ecx: r.read_u32()?,
                edx: r.read_u32()?,
            });
        }
        let device_count = r.read_u32()? as usize;
        if device_count > MAX_DEVICES {
            return Err(ConfigDecodeError::InvalidConfig(
                ConfigError::DeviceSetTooLarge,
            ));
        }
        let mut device_set = Vec::with_capacity(device_count);
        for _ in 0..device_count {
            device_set.push(r.read_u16()?);
        }
        if !r.is_finished() {
            return Err(ConfigDecodeError::TrailingBytes {
                trailing: r.remaining(),
            });
        }
        let config = Self {
            version,
            mem_bytes,
            vcpus,
            clock,
            base_image_hash,
            boot,
            epoch_len,
            hash_epochs,
            skid_margin: DEFAULT_SKID_MARGIN,
            resync_slack: DEFAULT_RESYNC_SLACK,
            cpuid_table,
            device_set,
        };
        config
            .validate()
            .map_err(ConfigDecodeError::InvalidConfig)?;
        Ok(config)
    }

    /// BLAKE3 of the canonical encoding (32 bytes) — `machine_config_hash`.
    pub fn config_hash(&self) -> Result<[u8; 32], ConfigError> {
        Ok(dh_inputlog::payload_digest(&self.canonical_encode()?))
    }
}

impl BootSpec {
    pub fn bzimage_boot_params_cmdline(&self) -> Result<Option<&[u8]>, ConfigError> {
        match self {
            BootSpec::Elf { .. } => Ok(None),
            BootSpec::BzImage { cmdline, .. } => {
                validate_canonical_bzimage_cmdline(cmdline)?;
                Ok(Some(cmdline))
            }
        }
    }
}

struct ConfigReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> ConfigReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8], ConfigDecodeError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(ConfigDecodeError::Truncated)?;
        let out = self
            .bytes
            .get(self.pos..end)
            .ok_or(ConfigDecodeError::Truncated)?;
        self.pos = end;
        Ok(out)
    }

    fn read_u8(&mut self) -> Result<u8, ConfigDecodeError> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, ConfigDecodeError> {
        Ok(u16::from_le_bytes(self.read_exact(2)?.try_into().unwrap()))
    }

    fn read_u32(&mut self) -> Result<u32, ConfigDecodeError> {
        Ok(u32::from_le_bytes(self.read_exact(4)?.try_into().unwrap()))
    }

    fn read_u64(&mut self) -> Result<u64, ConfigDecodeError> {
        Ok(u64::from_le_bytes(self.read_exact(8)?.try_into().unwrap()))
    }

    fn read_hash(&mut self) -> Result<[u8; 32], ConfigDecodeError> {
        Ok(self.read_exact(32)?.try_into().unwrap())
    }

    fn read_vec_u32(
        &mut self,
        field: &'static str,
        max: usize,
    ) -> Result<Vec<u8>, ConfigDecodeError> {
        let len = self.read_u32()? as usize;
        if len > max {
            return Err(ConfigDecodeError::LengthTooLarge { field, len, max });
        }
        Ok(self.read_exact(len)?.to_vec())
    }

    fn is_finished(&self) -> bool {
        self.pos == self.bytes.len()
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    UnsupportedVersion,
    MemNotAligned,
    VcpusNotOne,
    ZeroEpochLen,
    CmdlineTooLong,
    BzImageCmdlineNonAscii,
    BzImageCmdlineNul,
    BzImageCmdlineEmptyToken,
    BzImageCmdlineUnsupportedToken,
    BzImageCmdlineDuplicateToken,
    BzImageCmdlineBaselineOverride,
    BzImageCmdlineNotCanonical,
    CpuidTableTooLarge,
    DeviceSetTooLarge,
    CpuidTableUnsorted,
    DeviceSetUnsorted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigDecodeError {
    BadMagic,
    Truncated,
    TrailingBytes {
        trailing: usize,
    },
    BadBootTag {
        found: u8,
    },
    BadHashEpochs {
        found: u8,
    },
    ZeroClockTerm,
    LengthTooLarge {
        field: &'static str,
        len: usize,
        max: usize,
    },
    InvalidConfig(ConfigError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> MachineConfig {
        let mut c = MachineConfig::new(
            64 * 1024 * 1024,
            [0x11; 32],
            BootSpec::Elf {
                kernel_hash: [0x22; 32],
                cmdline: b"console=none".to_vec(),
            },
        );
        c.cpuid_table = vec![
            CpuidLeaf {
                function: 0,
                index: 0,
                flags: 0,
                eax: 0xD,
                ebx: 0,
                ecx: 0,
                edx: 0,
            },
            CpuidLeaf {
                function: 1,
                index: 0,
                flags: 0,
                eax: 0x0,
                ebx: 0,
                ecx: 0,
                edx: 0,
            },
        ];
        c.device_set = vec![1, 2, 3];
        c
    }

    #[test]
    fn golden_canonical_bytes() {
        // Frozen prefix of the v1 canonical encoding. If this test breaks,
        // you changed the machine_config_hash preimage — that invalidates
        // every stored snapshot ref. Bump `version` instead.
        let bytes = config().canonical_encode().unwrap();
        let mut expected = Vec::new();
        expected.extend_from_slice(b"DHMCFG\0\0");
        expected.extend_from_slice(&1u32.to_le_bytes()); // version
        expected.extend_from_slice(&(64u64 * 1024 * 1024).to_le_bytes());
        expected.extend_from_slice(&1u32.to_le_bytes()); // vcpus
        expected.extend_from_slice(&1u32.to_le_bytes()); // clock_num
        expected.extend_from_slice(&1u32.to_le_bytes()); // clock_den
        expected.extend_from_slice(&[0x11; 32]);
        expected.push(1); // ELF tag
        expected.extend_from_slice(&[0x22; 32]);
        expected.extend_from_slice(&12u32.to_le_bytes());
        expected.extend_from_slice(b"console=none");
        assert_eq!(&bytes[..expected.len()], &expected[..]);

        // Tail: epoch/hash_epochs/skid/resync/cpuid/devices.
        let tail = &bytes[expected.len()..];
        let mut exp_tail = Vec::new();
        exp_tail.extend_from_slice(&DEFAULT_EPOCH_LEN.to_le_bytes());
        exp_tail.push(1); // EPOCHS_ON — landing knobs are NOT encoded
        exp_tail.extend_from_slice(&2u32.to_le_bytes());
        exp_tail.extend_from_slice(&0u32.to_le_bytes()); // f=0
        exp_tail.extend_from_slice(&0u32.to_le_bytes()); // index
        exp_tail.extend_from_slice(&0u32.to_le_bytes()); // flags (bead nq5)
        exp_tail.extend_from_slice(&0xDu32.to_le_bytes());
        exp_tail.extend_from_slice(&[0u8; 12]); // ebx, ecx, edx
        exp_tail.extend_from_slice(&1u32.to_le_bytes()); // f=1
        exp_tail.extend_from_slice(&[0u8; 24]); // index/flags..edx all zero
        exp_tail.extend_from_slice(&3u32.to_le_bytes());
        exp_tail.extend_from_slice(&1u16.to_le_bytes());
        exp_tail.extend_from_slice(&2u16.to_le_bytes());
        exp_tail.extend_from_slice(&3u16.to_le_bytes());
        assert_eq!(tail, &exp_tail[..]);
    }

    #[test]
    fn hash_stable_and_field_sensitive() {
        let base = config();
        let h = base.config_hash().unwrap();
        assert_eq!(h, base.config_hash().unwrap());

        // Any field change must change the hash.
        let mut c = config();
        c.mem_bytes += MEM_ALIGN;
        assert_ne!(h, c.config_hash().unwrap());

        let mut c = config();
        c.clock = ClockRatio::new(2, 1).unwrap();
        assert_ne!(h, c.config_hash().unwrap());

        let mut c = config();
        c.hash_epochs = HashEpochs::FinalOnly;
        assert_ne!(h, c.config_hash().unwrap());

        let mut c = config();
        c.device_set = vec![1, 2, 4];
        assert_ne!(h, c.config_hash().unwrap());

        let mut c = config();
        if let BootSpec::Elf { cmdline, .. } = &mut c.boot {
            cmdline.push(b'x');
        }
        assert_ne!(h, c.config_hash().unwrap());
    }

    #[test]
    fn landing_knobs_do_not_fork_identity() {
        // ARCH §3.2: results are independent of the landing machinery, so
        // skid_margin / resync_slack must NOT change machine_config_hash.
        let h = config().config_hash().unwrap();
        let mut c = config();
        c.skid_margin = 16384;
        c.resync_slack = 4096;
        assert_eq!(h, c.config_hash().unwrap());
    }

    #[test]
    fn length_prefixes_prevent_ambiguity() {
        // Same concatenated bytes, different field split, must hash apart:
        // cmdline "ab" + device [] vs cmdline "a" + ... is structurally
        // impossible to confuse because of the length prefixes.
        let mk = |cmd: &[u8]| {
            let mut c = config();
            if let BootSpec::Elf { cmdline, .. } = &mut c.boot {
                *cmdline = cmd.to_vec();
            }
            c.config_hash().unwrap()
        };
        assert_ne!(mk(b"ab"), mk(b"a"));
        assert_ne!(mk(b""), mk(b"\0"));
    }

    #[test]
    fn bzimage_variant_encodes() {
        let mut c = config();
        c.boot = BootSpec::BzImage {
            kernel_hash: [0x33; 32],
            initramfs_hash: [0x44; 32],
            cmdline: canonicalize_bzimage_cmdline_extras(b"quiet").unwrap(),
        };
        let bytes = c.canonical_encode().unwrap();
        assert_ne!(
            bytes,
            config().canonical_encode().unwrap(),
            "boot variant must be domain-separated"
        );
        assert!(bytes.windows(32).any(|w| w == [0x44; 32]));
    }

    #[test]
    fn canonical_decode_round_trips_the_frozen_preimage() {
        let mut c = config();
        c.skid_margin = 16_384;
        c.resync_slack = 4_096;
        let bytes = c.canonical_encode().unwrap();
        let decoded = MachineConfig::canonical_decode(&bytes).unwrap();

        assert_eq!(decoded.canonical_encode().unwrap(), bytes);
        assert_eq!(decoded.version, c.version);
        assert_eq!(decoded.mem_bytes, c.mem_bytes);
        assert_eq!(decoded.vcpus, c.vcpus);
        assert_eq!(decoded.clock, c.clock);
        assert_eq!(decoded.base_image_hash, c.base_image_hash);
        assert_eq!(decoded.boot, c.boot);
        assert_eq!(decoded.epoch_len, c.epoch_len);
        assert_eq!(decoded.hash_epochs, c.hash_epochs);
        assert_eq!(decoded.cpuid_table, c.cpuid_table);
        assert_eq!(decoded.device_set, c.device_set);
        assert_eq!(decoded.skid_margin, DEFAULT_SKID_MARGIN);
        assert_eq!(decoded.resync_slack, DEFAULT_RESYNC_SLACK);
    }

    #[test]
    fn canonical_decode_round_trips_bzimage() {
        let mut c = config();
        c.boot = BootSpec::BzImage {
            kernel_hash: [0x33; 32],
            initramfs_hash: [0x44; 32],
            cmdline: canonicalize_bzimage_cmdline_extras(b"quiet").unwrap(),
        };
        c.hash_epochs = HashEpochs::FinalOnly;
        let bytes = c.canonical_encode().unwrap();
        let decoded = MachineConfig::canonical_decode(&bytes).unwrap();
        assert_eq!(decoded.canonical_encode().unwrap(), bytes);
        assert_eq!(decoded.boot, c.boot);
        assert_eq!(decoded.hash_epochs, HashEpochs::FinalOnly);
    }

    #[test]
    fn bzimage_cmdline_canonicalizes_allowed_extras() {
        assert_eq!(
            canonicalize_bzimage_cmdline_extras(b"").unwrap(),
            BZIMAGE_FORCED_CMDLINE
        );
        assert_eq!(
            canonicalize_bzimage_cmdline_extras(b"quiet").unwrap(),
            [BZIMAGE_FORCED_CMDLINE, b" quiet"].concat()
        );
        assert_eq!(
            canonicalize_bzimage_cmdline_extras(b"loglevel=4 quiet").unwrap(),
            [BZIMAGE_FORCED_CMDLINE, b" quiet loglevel=4"].concat(),
            "caller order normalizes to whitelist order"
        );
        assert_eq!(
            canonicalize_bzimage_cmdline_extras(b"quiet\tloglevel=7").unwrap(),
            [BZIMAGE_FORCED_CMDLINE, b" quiet loglevel=7"].concat()
        );
    }

    #[test]
    fn bzimage_cmdline_rejects_unsupported_duplicates_and_baseline_tokens() {
        assert_eq!(
            canonicalize_bzimage_cmdline_extras(b"root=/dev/vda"),
            Err(ConfigError::BzImageCmdlineUnsupportedToken)
        );
        assert_eq!(
            canonicalize_bzimage_cmdline_extras(b"quiet quiet"),
            Err(ConfigError::BzImageCmdlineDuplicateToken)
        );
        assert_eq!(
            canonicalize_bzimage_cmdline_extras(b"loglevel=3 loglevel=4"),
            Err(ConfigError::BzImageCmdlineDuplicateToken)
        );
        assert_eq!(
            canonicalize_bzimage_cmdline_extras(b"console=ttyS0"),
            Err(ConfigError::BzImageCmdlineBaselineOverride)
        );
        assert_eq!(
            canonicalize_bzimage_cmdline_extras(b"nokaslr"),
            Err(ConfigError::BzImageCmdlineBaselineOverride)
        );
        assert_eq!(
            canonicalize_bzimage_cmdline_extras(b"loglevel=8"),
            Err(ConfigError::BzImageCmdlineUnsupportedToken)
        );
        assert_eq!(
            canonicalize_bzimage_cmdline_extras(b"quiet  loglevel=3"),
            Err(ConfigError::BzImageCmdlineEmptyToken)
        );
        assert_eq!(
            canonicalize_bzimage_cmdline_extras(b"quiet\0"),
            Err(ConfigError::BzImageCmdlineNul)
        );
        assert_eq!(
            canonicalize_bzimage_cmdline_extras(b"quiet \xff"),
            Err(ConfigError::BzImageCmdlineNonAscii)
        );
    }

    #[test]
    fn bzimage_config_requires_canonical_cmdline() {
        let mut c = config();
        c.boot = BootSpec::BzImage {
            kernel_hash: [0x33; 32],
            initramfs_hash: [0x44; 32],
            cmdline: b"quiet".to_vec(),
        };
        assert_eq!(c.validate(), Err(ConfigError::BzImageCmdlineNotCanonical));

        c.boot = BootSpec::BzImage {
            kernel_hash: [0x33; 32],
            initramfs_hash: [0x44; 32],
            cmdline: [BZIMAGE_FORCED_CMDLINE, b" loglevel=4 quiet"].concat(),
        };
        assert_eq!(c.validate(), Err(ConfigError::BzImageCmdlineNotCanonical));
    }

    #[test]
    fn bzimage_cmdline_max_is_checked_after_composition() {
        let baseline = vec![b'x'; MAX_CMDLINE + 1];
        assert_eq!(
            compose_bzimage_cmdline(&baseline, BzImageCmdlineExtras::default()),
            Err(ConfigError::CmdlineTooLong)
        );
    }

    #[test]
    fn bzimage_hash_preimage_cmdline_matches_boot_params_bytes() {
        let canonical = canonicalize_bzimage_cmdline_extras(b"loglevel=3 quiet").unwrap();
        let mut c = config();
        c.boot = BootSpec::BzImage {
            kernel_hash: [0x33; 32],
            initramfs_hash: [0x44; 32],
            cmdline: canonical.clone(),
        };
        let bytes = c.canonical_encode().unwrap();
        let cmdline_len_off = 8 + 4 + 8 + 4 + 4 + 4 + 32 + 1 + 32 + 32;
        let len = u32::from_le_bytes(
            bytes[cmdline_len_off..cmdline_len_off + 4]
                .try_into()
                .unwrap(),
        ) as usize;
        let cmdline_off = cmdline_len_off + 4;
        let preimage_cmdline = &bytes[cmdline_off..cmdline_off + len];
        let boot_params_cmdline = c
            .boot
            .bzimage_boot_params_cmdline()
            .unwrap()
            .expect("BzImage cmdline");
        assert_eq!(preimage_cmdline, canonical);
        assert_eq!(boot_params_cmdline, canonical);
        assert_eq!(preimage_cmdline, boot_params_cmdline);
    }

    #[test]
    fn canonical_decode_rejects_non_canonical_bytes() {
        assert_eq!(
            MachineConfig::canonical_decode(b"not-mcfg"),
            Err(ConfigDecodeError::BadMagic)
        );

        let mut bytes = config().canonical_encode().unwrap();
        bytes.push(0);
        assert_eq!(
            MachineConfig::canonical_decode(&bytes),
            Err(ConfigDecodeError::TrailingBytes { trailing: 1 })
        );

        let mut bytes = config().canonical_encode().unwrap();
        let boot_tag = 8 + 4 + 8 + 4 + 4 + 4 + 32;
        bytes[boot_tag] = 9;
        assert_eq!(
            MachineConfig::canonical_decode(&bytes),
            Err(ConfigDecodeError::BadBootTag { found: 9 })
        );

        let mut bytes = config().canonical_encode().unwrap();
        let hash_epochs = bytes
            .windows(DEFAULT_EPOCH_LEN.to_le_bytes().len())
            .position(|w| w == DEFAULT_EPOCH_LEN.to_le_bytes())
            .unwrap()
            + DEFAULT_EPOCH_LEN.to_le_bytes().len();
        bytes[hash_epochs] = 7;
        assert_eq!(
            MachineConfig::canonical_decode(&bytes),
            Err(ConfigDecodeError::BadHashEpochs { found: 7 })
        );

        let mut bytes = config().canonical_encode().unwrap();
        bytes[8..12].copy_from_slice(&2u32.to_le_bytes());
        assert_eq!(
            MachineConfig::canonical_decode(&bytes),
            Err(ConfigDecodeError::InvalidConfig(
                ConfigError::UnsupportedVersion
            ))
        );
    }

    #[test]
    fn validation_rejects_bad_configs() {
        let mut c = config();
        c.mem_bytes = MEM_ALIGN + 1;
        assert_eq!(c.canonical_encode(), Err(ConfigError::MemNotAligned));

        let mut c = config();
        c.vcpus = 2;
        assert_eq!(c.validate(), Err(ConfigError::VcpusNotOne));

        let mut c = config();
        c.epoch_len = 0;
        assert_eq!(c.validate(), Err(ConfigError::ZeroEpochLen));

        let mut c = config();
        c.cpuid_table.swap(0, 1);
        assert_eq!(c.validate(), Err(ConfigError::CpuidTableUnsorted));

        let mut c = config();
        c.device_set = vec![2, 2];
        assert_eq!(c.validate(), Err(ConfigError::DeviceSetUnsorted));

        let mut c = config();
        if let BootSpec::Elf { cmdline, .. } = &mut c.boot {
            *cmdline = vec![0u8; MAX_CMDLINE + 1];
        }
        assert_eq!(c.validate(), Err(ConfigError::CmdlineTooLong));

        let mut c = config();
        c.device_set = (0..=(MAX_DEVICES as u16)).collect();
        assert_eq!(c.validate(), Err(ConfigError::DeviceSetTooLarge));
    }

    #[test]
    fn entropy_seed_is_not_part_of_the_preimage() {
        // By construction: MachineConfig has no entropy_seed field. This
        // test pins the invariant in prose — if you are adding one, stop:
        // the determinism class must not vary per seed (bead u49 / ARCH §5).
        let names = [
            "version",
            "mem_bytes",
            "vcpus",
            "clock",
            "base_image_hash",
            "boot",
            "epoch_len",
            "hash_epochs",
            "skid_margin",
            "resync_slack",
            "cpuid_table",
            "device_set",
        ];
        assert_eq!(names.len(), 12);
    }
}
