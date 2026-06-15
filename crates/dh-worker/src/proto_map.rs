//! Domain ↔ proto enum bridges (bead sr5). HAND-WRITTEN MATCHES ONLY.
//!
//! The two enum families disagree in offset AND order — domain
//! `SlotState::Running` is discriminant 1 while proto `RUNNING = 3`, and
//! proto reserves 0 for `*_UNSPECIFIED` which the domain enums never
//! carry — so an `as i32` cast on a domain enum is ALWAYS a bug. Every
//! crossing goes through these functions; the unit tests pin each arm to
//! the exact proto wire number so a renumbering on either side breaks
//! loudly here instead of silently mislabeling slots on the API.
//!
//! Exhaustiveness is the other half of the contract: when run control
//! grows a `runctl::StopReason::Faulted` producer, the match below stops
//! compiling and forces the mapping decision at the same commit — never
//! a silent `_ => Unspecified`. (`NextSdkEvent` landed exactly that way
//! with bead 4qo.)

use dh_proto::v1 as proto;
use dh_vmm::config::{BootSpec, ConfigError, CpuidLeaf, HashEpochs, MachineConfig};
use dh_vmm::{vt::ClockRatio, SlotState};

/// Slot lifecycle → API.md §2.8 `SlotState` (the `_S` suffixes are the
/// proto package's enum-value collision convention, not semantics).
pub fn slot_state_to_proto(s: SlotState) -> proto::SlotState {
    match s {
        SlotState::Empty => proto::SlotState::Empty,
        SlotState::Paused => proto::SlotState::PausedS,
        SlotState::Running => proto::SlotState::Running,
        SlotState::Frozen => proto::SlotState::Frozen,
        SlotState::Faulted => proto::SlotState::FaultedS,
    }
}

/// Slot-manager lease → wire `Lease` (API.md §2.1).
pub fn lease_to_proto(l: &crate::slot_manager::Lease) -> proto::Lease {
    proto::Lease {
        slot_id: l.slot_id,
        token: l.token.to_vec(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MachineConfigWireError {
    MissingBoot,
    BadHash { field: &'static str, len: usize },
    BadHashEpochs(i32),
    BadDeviceId(u32),
    ZeroClockTerm,
    InvalidConfig(ConfigError),
}

impl std::fmt::Display for MachineConfigWireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MachineConfigWireError::MissingBoot => write!(f, "missing boot spec"),
            MachineConfigWireError::BadHash { field, len } => {
                write!(f, "{field} must be 32 bytes, got {len}")
            }
            MachineConfigWireError::BadHashEpochs(value) => {
                write!(f, "unknown hash_epochs value {value}")
            }
            MachineConfigWireError::BadDeviceId(id) => {
                write!(f, "device id {id:#x} does not fit u16")
            }
            MachineConfigWireError::ZeroClockTerm => {
                write!(f, "clock numerator/denominator must be nonzero")
            }
            MachineConfigWireError::InvalidConfig(e) => write!(f, "invalid MachineConfig: {e:?}"),
        }
    }
}

impl std::error::Error for MachineConfigWireError {}

impl From<ConfigError> for MachineConfigWireError {
    fn from(e: ConfigError) -> Self {
        MachineConfigWireError::InvalidConfig(e)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForkRequestWireError {
    EntropySeedCountMismatch { count: u32, seeds: usize },
    BadEntropySeed { index: usize, len: usize },
}

impl std::fmt::Display for ForkRequestWireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForkRequestWireError::EntropySeedCountMismatch { count, seeds } => {
                write!(
                    f,
                    "fork entropy_seeds must be empty or match count {count}, got {seeds}"
                )
            }
            ForkRequestWireError::BadEntropySeed { index, len } => {
                write!(f, "fork entropy_seeds[{index}] must be 32 bytes, got {len}")
            }
        }
    }
}

impl std::error::Error for ForkRequestWireError {}

/// ForkRequest entropy contract (API.md §2.2): absent seeds continue the
/// fork-point PRNG for every child. When present, there is one 32-byte entry
/// per child; an all-zero entry also means "continue", and a non-zero entry
/// starts that child segment from the given seed. Slot-manager capacity checks
/// own the public `count` range before RPC wiring calls this helper.
pub fn fork_entropy_seeds_from_proto(
    count: u32,
    entropy_seeds: &[Vec<u8>],
) -> Result<Vec<Option<[u8; 32]>>, ForkRequestWireError> {
    if entropy_seeds.is_empty() {
        return Ok(vec![None; count as usize]);
    }
    if entropy_seeds.len() != count as usize {
        return Err(ForkRequestWireError::EntropySeedCountMismatch {
            count,
            seeds: entropy_seeds.len(),
        });
    }
    entropy_seeds
        .iter()
        .enumerate()
        .map(|(index, seed)| {
            let seed: [u8; 32] =
                seed.as_slice()
                    .try_into()
                    .map_err(|_| ForkRequestWireError::BadEntropySeed {
                        index,
                        len: seed.len(),
                    })?;
            Ok((seed != [0u8; 32]).then_some(seed))
        })
        .collect()
}

pub fn machine_config_to_proto(config: &MachineConfig) -> proto::MachineConfig {
    proto::MachineConfig {
        version: config.version,
        mem_bytes: config.mem_bytes,
        vcpus: config.vcpus,
        clock_num: config.clock.num(),
        clock_den: config.clock.den(),
        base_image_hash: config.base_image_hash.to_vec(),
        boot: Some(boot_spec_to_proto(&config.boot)),
        epoch_len: config.epoch_len,
        hash_epochs: i32::from(match config.hash_epochs {
            HashEpochs::EpochsOn => proto::HashEpochs::EpochsOn,
            HashEpochs::FinalOnly => proto::HashEpochs::FinalOnly,
        }),
        skid_margin: config.skid_margin,
        cpuid_table: config.cpuid_table.iter().map(cpuid_leaf_to_proto).collect(),
        device_set: config.device_set.iter().map(|id| u32::from(*id)).collect(),
    }
}

pub fn machine_config_from_proto(
    config: &proto::MachineConfig,
) -> Result<MachineConfig, MachineConfigWireError> {
    let clock = ClockRatio::new(config.clock_num, config.clock_den)
        .ok_or(MachineConfigWireError::ZeroClockTerm)?;
    let boot = match config
        .boot
        .as_ref()
        .and_then(|boot| boot.kind.as_ref())
        .ok_or(MachineConfigWireError::MissingBoot)?
    {
        proto::boot_spec::Kind::Elf(elf) => BootSpec::Elf {
            kernel_hash: hash32("boot.elf.kernel_hash", &elf.kernel_hash)?,
            cmdline: elf.cmdline.clone(),
        },
        proto::boot_spec::Kind::Bzimage(bzimage) => BootSpec::BzImage {
            kernel_hash: hash32("boot.bzimage.kernel_hash", &bzimage.kernel_hash)?,
            initramfs_hash: hash32("boot.bzimage.initramfs_hash", &bzimage.initramfs_hash)?,
            cmdline: bzimage.cmdline.clone(),
        },
    };
    let hash_epochs = match proto::HashEpochs::try_from(config.hash_epochs)
        .map_err(|_| MachineConfigWireError::BadHashEpochs(config.hash_epochs))?
    {
        proto::HashEpochs::EpochsOn => HashEpochs::EpochsOn,
        proto::HashEpochs::FinalOnly => HashEpochs::FinalOnly,
        proto::HashEpochs::Unspecified => {
            return Err(MachineConfigWireError::BadHashEpochs(config.hash_epochs));
        }
    };
    let cpuid_table = config
        .cpuid_table
        .iter()
        .map(cpuid_leaf_from_proto)
        .collect();
    let device_set = config
        .device_set
        .iter()
        .copied()
        .map(|id| u16::try_from(id).map_err(|_| MachineConfigWireError::BadDeviceId(id)))
        .collect::<Result<Vec<_>, _>>()?;
    let out = MachineConfig {
        version: config.version,
        mem_bytes: config.mem_bytes,
        vcpus: config.vcpus,
        clock,
        base_image_hash: hash32("base_image_hash", &config.base_image_hash)?,
        boot,
        epoch_len: config.epoch_len,
        hash_epochs,
        skid_margin: config.skid_margin,
        resync_slack: dh_vmm::config::DEFAULT_RESYNC_SLACK,
        cpuid_table,
        device_set,
    };
    out.validate()?;
    Ok(out)
}

fn boot_spec_to_proto(boot: &BootSpec) -> proto::BootSpec {
    proto::BootSpec {
        kind: Some(match boot {
            BootSpec::Elf {
                kernel_hash,
                cmdline,
            } => proto::boot_spec::Kind::Elf(proto::ElfBoot {
                kernel_hash: kernel_hash.to_vec(),
                cmdline: cmdline.clone(),
            }),
            BootSpec::BzImage {
                kernel_hash,
                initramfs_hash,
                cmdline,
            } => proto::boot_spec::Kind::Bzimage(proto::BzImageBoot {
                kernel_hash: kernel_hash.to_vec(),
                initramfs_hash: initramfs_hash.to_vec(),
                cmdline: cmdline.clone(),
            }),
        }),
    }
}

fn cpuid_leaf_to_proto(leaf: &CpuidLeaf) -> proto::CpuidLeaf {
    proto::CpuidLeaf {
        function: leaf.function,
        index: leaf.index,
        flags: leaf.flags,
        eax: leaf.eax,
        ebx: leaf.ebx,
        ecx: leaf.ecx,
        edx: leaf.edx,
    }
}

fn cpuid_leaf_from_proto(leaf: &proto::CpuidLeaf) -> CpuidLeaf {
    CpuidLeaf {
        function: leaf.function,
        index: leaf.index,
        flags: leaf.flags,
        eax: leaf.eax,
        ebx: leaf.ebx,
        ecx: leaf.ecx,
        edx: leaf.edx,
    }
}

fn hash32(field: &'static str, bytes: &[u8]) -> Result<[u8; 32], MachineConfigWireError> {
    bytes
        .try_into()
        .map_err(|_| MachineConfigWireError::BadHash {
            field,
            len: bytes.len(),
        })
}

/// Slot-manager introspection row → wire `SlotInfo` (API.md §2.8). The
/// state field is a prost open-enum i32 — `i32::from` on the PROTO enum
/// (never a domain cast; the deny test below holds the line).
pub fn slot_info_to_proto(i: &crate::slot_manager::SlotInfo) -> proto::SlotInfo {
    proto::SlotInfo {
        slot_id: i.slot_id,
        state: i32::from(slot_state_to_proto(i.state)),
        icount: i.icount,
        base: i.base_snapshot_id.map(|hash| proto::SnapshotRef {
            hash: hash.to_vec(),
        }),
        live_children: i.live_children,
    }
}

/// Segment stop → API.md §2.4 `StopReason`. runctl deliberately has no
/// `Faulted` (fault wiring is run control's, see sr5 notes) — that arm
/// appears here the day the variant does.
#[cfg(target_arch = "x86_64")]
pub fn stop_reason_to_proto(r: dh_vmm::runctl::StopReason) -> proto::StopReason {
    use dh_vmm::runctl::StopReason as R;
    match r {
        R::BudgetReached => proto::StopReason::BudgetReached,
        R::GoalSatisfied => proto::StopReason::GoalSatisfied,
        R::NextSdkEvent => proto::StopReason::NextSdkEvent,
        R::HardCap => proto::StopReason::HardCap,
        R::Paused => proto::StopReason::Paused,
        R::GuestHalted => proto::StopReason::GuestHalted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every arm pinned to the exact wire number — the order-divergence
    /// trap (domain Running=1 vs proto RUNNING=3) is the reason this
    /// module exists.
    #[test]
    fn slot_state_wire_numbers_are_pinned() {
        let pins = [
            (SlotState::Empty, 1),
            (SlotState::Paused, 2),
            (SlotState::Running, 3),
            (SlotState::Frozen, 4),
            (SlotState::Faulted, 5),
        ];
        for (domain, wire) in pins {
            assert_eq!(
                slot_state_to_proto(domain) as i32,
                wire,
                "{domain:?} wire number"
            );
        }
        // The trap itself, demonstrated: the naive cast lies for four of
        // the five states (it agrees on Paused=2 by pure coincidence —
        // which is exactly what makes the bug class survive spot checks).
        let lying_casts = pins.iter().filter(|(d, w)| (*d as i32) != *w).count();
        assert_eq!(lying_casts, 4, "the order-divergence trap moved");
    }

    /// Cross-pin: the DHILOG END byte (dh_vmm::recording::stop_reason_u8,
    /// which cannot see dh-proto) must agree with the proto mapping for
    /// every variant — the §3.3 "mirrors proto StopReason" coupling.
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn recording_end_byte_agrees_with_the_proto_mapping() {
        use dh_vmm::runctl::StopReason as R;
        for r in [
            R::BudgetReached,
            R::GoalSatisfied,
            R::NextSdkEvent,
            R::HardCap,
            R::Paused,
            R::GuestHalted,
        ] {
            assert_eq!(
                i32::from(dh_vmm::recording::stop_reason_u8(r)),
                stop_reason_to_proto(r) as i32,
                "{r:?}"
            );
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn stop_reason_wire_numbers_are_pinned() {
        use dh_vmm::runctl::StopReason as R;
        let pins = [
            (R::BudgetReached, 1),
            (R::GoalSatisfied, 2),
            (R::NextSdkEvent, 3),
            (R::HardCap, 4),
            (R::Paused, 5),
            (R::GuestHalted, 6),
        ];
        for (domain, wire) in pins {
            assert_eq!(
                stop_reason_to_proto(domain) as i32,
                wire,
                "{domain:?} wire number"
            );
        }
    }

    /// The §2.8 introspection crossings carry every field, and the state
    /// travels through the pinned mapping (never a domain cast).
    #[test]
    fn slot_info_and_lease_cross_with_every_field() {
        let lease = crate::slot_manager::Lease {
            slot_id: 3,
            token: [0xA5; 16],
        };
        let wire = lease_to_proto(&lease);
        assert_eq!(wire.slot_id, 3);
        assert_eq!(wire.token, vec![0xA5; 16]);

        let info = crate::slot_manager::SlotInfo {
            slot_id: 2,
            state: SlotState::Frozen,
            icount: 7_000_000,
            base_snapshot_id: Some([0xBC; 32]),
            live_children: 4,
        };
        let wire = slot_info_to_proto(&info);
        assert_eq!(wire.slot_id, 2);
        assert_eq!(wire.state, 4, "Frozen through the pinned mapping");
        assert_eq!(wire.icount, 7_000_000);
        assert_eq!(wire.base.as_ref().unwrap().hash, vec![0xBC; 32]);
        assert_eq!(wire.live_children, 4);

        let bare = crate::slot_manager::SlotInfo {
            base_snapshot_id: None,
            ..info
        };
        assert_eq!(slot_info_to_proto(&bare).base, None, "no segment yet");
    }

    #[test]
    fn machine_config_wire_shape_is_lossless_for_canonical_fields() {
        let config = full_machine_config();
        let wire = machine_config_to_proto(&config);
        assert_eq!(wire.cpuid_table.len(), 2);
        assert_eq!(wire.device_set, vec![1, 4, 7]);
        assert_eq!(wire.skid_margin, 16_384);
        let back = machine_config_from_proto(&wire).unwrap();

        assert_eq!(
            back.canonical_encode().unwrap(),
            config.canonical_encode().unwrap()
        );
        assert_eq!(back.cpuid_table, config.cpuid_table);
        assert_eq!(back.device_set, config.device_set);
        assert_eq!(back.resync_slack, dh_vmm::config::DEFAULT_RESYNC_SLACK);
    }

    #[test]
    fn machine_config_wire_rejects_lossy_or_invalid_shapes() {
        let mut wire = machine_config_to_proto(&full_machine_config());
        wire.base_image_hash.pop();
        assert!(matches!(
            machine_config_from_proto(&wire),
            Err(MachineConfigWireError::BadHash {
                field: "base_image_hash",
                len: 31
            })
        ));

        let mut wire = machine_config_to_proto(&full_machine_config());
        if let Some(proto::boot_spec::Kind::Bzimage(bzimage)) =
            wire.boot.as_mut().and_then(|boot| boot.kind.as_mut())
        {
            bzimage.kernel_hash.pop();
        }
        assert!(matches!(
            machine_config_from_proto(&wire),
            Err(MachineConfigWireError::BadHash {
                field: "boot.bzimage.kernel_hash",
                len: 31
            })
        ));

        let mut wire = machine_config_to_proto(&full_machine_config());
        wire.hash_epochs = proto::HashEpochs::Unspecified as i32;
        assert_eq!(
            machine_config_from_proto(&wire),
            Err(MachineConfigWireError::BadHashEpochs(0))
        );

        let mut wire = machine_config_to_proto(&full_machine_config());
        wire.hash_epochs = 99;
        assert_eq!(
            machine_config_from_proto(&wire),
            Err(MachineConfigWireError::BadHashEpochs(99))
        );

        let mut wire = machine_config_to_proto(&full_machine_config());
        wire.cpuid_table.swap(0, 1);
        assert_eq!(
            machine_config_from_proto(&wire),
            Err(MachineConfigWireError::InvalidConfig(
                ConfigError::CpuidTableUnsorted
            ))
        );

        let mut wire = machine_config_to_proto(&full_machine_config());
        wire.device_set.swap(0, 1);
        assert_eq!(
            machine_config_from_proto(&wire),
            Err(MachineConfigWireError::InvalidConfig(
                ConfigError::DeviceSetUnsorted
            ))
        );

        let mut wire = machine_config_to_proto(&full_machine_config());
        wire.device_set = vec![u32::from(u16::MAX) + 1];
        assert_eq!(
            machine_config_from_proto(&wire),
            Err(MachineConfigWireError::BadDeviceId(u32::from(u16::MAX) + 1))
        );

        let mut wire = machine_config_to_proto(&full_machine_config());
        wire.boot = None;
        assert_eq!(
            machine_config_from_proto(&wire),
            Err(MachineConfigWireError::MissingBoot)
        );
    }

    #[test]
    fn fork_entropy_seed_contract_normalizes_and_rejects_bad_shapes() {
        assert_eq!(
            fork_entropy_seeds_from_proto(3, &[]).unwrap(),
            vec![None, None, None]
        );

        let explicit = fork_entropy_seeds_from_proto(
            2,
            &[vec![0; 32], {
                let mut seed = vec![0xA7; 32];
                seed[0] = 1;
                seed
            }],
        )
        .unwrap();
        assert_eq!(explicit[0], None);
        assert_eq!(
            explicit[1],
            Some({
                let mut seed = [0xA7; 32];
                seed[0] = 1;
                seed
            })
        );

        assert_eq!(
            fork_entropy_seeds_from_proto(2, &[vec![0; 32]]),
            Err(ForkRequestWireError::EntropySeedCountMismatch { count: 2, seeds: 1 })
        );
        assert_eq!(
            fork_entropy_seeds_from_proto(1, &[vec![0; 31]]),
            Err(ForkRequestWireError::BadEntropySeed { index: 0, len: 31 })
        );
        assert_eq!(
            fork_entropy_seeds_from_proto(1, &[vec![0; 33]]),
            Err(ForkRequestWireError::BadEntropySeed { index: 0, len: 33 })
        );
    }

    fn full_machine_config() -> MachineConfig {
        let mut config = MachineConfig::new(
            64 * 1024 * 1024,
            [0x11; 32],
            BootSpec::BzImage {
                kernel_hash: [0x22; 32],
                initramfs_hash: [0x33; 32],
                cmdline: b"console=ttyS0".to_vec(),
            },
        );
        config.clock = ClockRatio::new(1000, 1).unwrap();
        config.hash_epochs = HashEpochs::FinalOnly;
        config.skid_margin = 16_384;
        config.resync_slack = 4_096;
        config.cpuid_table = vec![
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
                eax: 1,
                ebx: 2,
                ecx: 3,
                edx: 4,
            },
        ];
        config.device_set = vec![1, 4, 7];
        config
    }

    /// The iteration-80 deny gate (sr5 review, armed by ol1's SlotInfo
    /// serving): a domain-enum `as i32` silently mislabels four of five
    /// slot states on the wire, so OUTSIDE this module no dh-worker
    /// source may contain `as i32` at all — every enum crossing comes
    /// here. (Same shape as dh-devices' no_host_ambient_authority gate.)
    #[test]
    fn no_enum_casts_outside_proto_map() {
        fn walk(dir: &std::path::Path, hits: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, hits);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") || path.ends_with("proto_map.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                for (lineno, line) in text.lines().enumerate() {
                    if line.contains("as i32") {
                        hits.push(format!("{}:{}: {line}", path.display(), lineno + 1));
                    }
                }
            }
        }
        let mut hits = Vec::new();
        walk(
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
            &mut hits,
        );
        assert!(
            hits.is_empty(),
            "`as i32` outside proto_map.rs — domain enums must cross through \
             the pinned mappings (SlotState::Running as i32 == 1, proto \
             RUNNING == 3):\n{}",
            hits.join("\n")
        );
    }
}
