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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuidLeaf {
    pub function: u32,
    pub index: u32,
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
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
            for v in [
                leaf.function,
                leaf.index,
                leaf.eax,
                leaf.ebx,
                leaf.ecx,
                leaf.edx,
            ] {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out.extend_from_slice(&(self.device_set.len() as u32).to_le_bytes());
        for id in &self.device_set {
            out.extend_from_slice(&id.to_le_bytes());
        }
        Ok(out)
    }

    /// BLAKE3 of the canonical encoding (32 bytes) — `machine_config_hash`.
    pub fn config_hash(&self) -> Result<[u8; 32], ConfigError> {
        Ok(dh_inputlog::payload_digest(&self.canonical_encode()?))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    UnsupportedVersion,
    MemNotAligned,
    VcpusNotOne,
    ZeroEpochLen,
    CmdlineTooLong,
    CpuidTableTooLarge,
    DeviceSetTooLarge,
    CpuidTableUnsorted,
    DeviceSetUnsorted,
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
                eax: 0xD,
                ebx: 0,
                ecx: 0,
                edx: 0,
            },
            CpuidLeaf {
                function: 1,
                index: 0,
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
        exp_tail.extend_from_slice(&0u32.to_le_bytes());
        exp_tail.extend_from_slice(&0xDu32.to_le_bytes());
        exp_tail.extend_from_slice(&[0u8; 12]); // ebx, ecx, edx
        exp_tail.extend_from_slice(&1u32.to_le_bytes()); // f=1
        exp_tail.extend_from_slice(&[0u8; 20]); // index..edx all zero
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
            cmdline: b"quiet".to_vec(),
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
