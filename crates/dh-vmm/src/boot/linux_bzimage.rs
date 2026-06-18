//! Host-side Linux bzImage setup-header parser, deterministic subset
//! validator, and boot-parameter layout planner. This module performs no KVM
//! work; it only turns kernel bytes and artifact sizes into checked bytes and
//! GPAs for the later Linux loader.

use crate::config;
use crate::kvm::{MMIO_HOLE_BASE, MMIO_HOLE_LEN};

use super::{BOOTINFO_GPA, PD_BASE_GPA, PML4_GPA};

const SETUP_SECTS_OFF: usize = 0x1f1;
const SETUP_HEADER_LEN_OFF: usize = 0x201;
const HEADER_MAGIC_OFF: usize = 0x202;
const PROTOCOL_VERSION_OFF: usize = 0x206;
const TYPE_OF_LOADER_OFF: usize = 0x210;
const LOADFLAGS_OFF: usize = 0x211;
const CODE32_START_OFF: usize = 0x214;
const RAMDISK_IMAGE_OFF: usize = 0x218;
const RAMDISK_SIZE_OFF: usize = 0x21c;
const HEAP_END_PTR_OFF: usize = 0x224;
const CMD_LINE_PTR_OFF: usize = 0x228;
const INITRD_ADDR_MAX_OFF: usize = 0x22c;
const KERNEL_ALIGNMENT_OFF: usize = 0x230;
const RELOCATABLE_KERNEL_OFF: usize = 0x234;
const XLOADFLAGS_OFF: usize = 0x236;
const CMDLINE_SIZE_OFF: usize = 0x238;
const HARDWARE_SUBARCH_OFF: usize = 0x23c;
const HARDWARE_SUBARCH_DATA_OFF: usize = 0x240;
const PAYLOAD_OFFSET_OFF: usize = 0x248;
const PAYLOAD_LENGTH_OFF: usize = 0x24c;
const SETUP_DATA_OFF: usize = 0x250;
const PREF_ADDRESS_OFF: usize = 0x258;
const INIT_SIZE_OFF: usize = 0x260;
const SETUP_HEADER_END: usize = 0x268;

const HEADER_MAGIC: &[u8; 4] = b"HdrS";
const MIN_PROTOCOL_VERSION: u16 = 0x020a;
const SECTOR: u64 = 512;
const DEFAULT_SETUP_SECTS: u8 = 4;
const LOADFLAGS_LOADED_HIGH: u8 = 0x01;
const LOADFLAGS_CAN_USE_HEAP: u8 = 0x80;
const XLF_KERNEL_64: u16 = 0x0001;
const XLF_CAN_BE_LOADED_ABOVE_4G: u16 = 0x0002;
const XLF_EFI_HANDOVER_32: u16 = 0x0004;
const XLF_EFI_HANDOVER_64: u16 = 0x0008;
const XLF_EFI_KEXEC: u16 = 0x0010;
const XLF_5LEVEL: u16 = 0x0020;
const XLF_5LEVEL_ENABLED: u16 = 0x0040;
#[cfg(test)]
const XLF_MEM_ENCRYPTION: u16 = 0x0080;
const XLOADFLAGS_SUPPORTED: u16 = XLF_KERNEL_64
    | XLF_CAN_BE_LOADED_ABOVE_4G
    | XLF_EFI_HANDOVER_32
    | XLF_EFI_HANDOVER_64
    | XLF_EFI_KEXEC
    | XLF_5LEVEL
    | XLF_5LEVEL_ENABLED;
const MIN_KERNEL_ALIGNMENT: u32 = 0x20_0000;
const PAGE_SIZE: u64 = 4096;

pub const LINUX_BOOT_PARAMS_SIZE: usize = 4096;
pub const LINUX_BOOT_PARAMS_GPA: u64 = BOOTINFO_GPA;
pub const LINUX_CMDLINE_GPA: u64 = 0x8000;
pub const LINUX_CMDLINE_RESERVED_LEN: u64 = 0x2000;
pub const LINUX_KERNEL_LOAD_GPA: u64 = 0x10_0000;
pub const LINUX_LEGACY_IO_HOLE_BASE: u64 = 0x0a_0000;
pub const LINUX_LEGACY_IO_HOLE_LEN: u64 = 0x06_0000;
pub const LINUX_APIC_MMIO_BASE: u64 = 0xfee0_0000;
pub const LINUX_APIC_MMIO_LEN: u64 = 0x1000;

const LINUX_PAGE_TABLES_GPA: u64 = PML4_GPA;
const LINUX_PAGE_TABLES_LEN: u64 = (PD_BASE_GPA + 4 * PAGE_SIZE) - PML4_GPA;
const LINUX_LOW_RESERVED_LEN: u64 = LINUX_CMDLINE_GPA + LINUX_CMDLINE_RESERVED_LEN;
const LINUX_HEAP_END_PTR: u16 = 0xde00;
const LINUX_TYPE_OF_LOADER: u8 = 0xff;
const ZERO_PAGE_E820_COUNT_OFF: usize = 0x1e8;
const ZERO_PAGE_E820_TABLE_OFF: usize = 0x2d0;
const E820_ENTRY_SIZE: usize = 20;
const E820_TABLE_CAP: usize = 128;
const E820_RAM: u32 = 1;
const E820_RESERVED: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BzImageLayout {
    pub setup_header: Vec<u8>,
    pub protocol_version: u16,
    pub setup_sects: u8,
    pub setup_bytes: u64,
    pub kernel_image_file_offset: u64,
    pub kernel_image_length: u64,
    pub payload_file_offset: u64,
    pub payload_length: u64,
    pub init_size: u64,
    pub loadflags: u8,
    pub xloadflags: u16,
    pub kernel_alignment: u32,
    pub relocatable_kernel: bool,
    pub pref_address: u64,
    pub initrd_addr_max: u32,
    pub cmdline_size: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BzImageError {
    TruncatedSetupHeader {
        len: usize,
        min: usize,
    },
    BadHeaderMagic {
        found: [u8; 4],
    },
    UnsupportedProtocolVersion {
        found: u16,
        min: u16,
    },
    UnsupportedLoadFlags {
        loadflags: u8,
    },
    UnsupportedXloadflags {
        xloadflags: u16,
        supported: u16,
    },
    PayloadOverflow,
    PayloadOutsideImage {
        start: u64,
        len: u64,
        image_len: u64,
    },
    SetupHeaderTooShort {
        end: usize,
        min: usize,
    },
    BadInitSize {
        init_size: u32,
    },
    BadKernelAlignment {
        alignment: u32,
    },
    UnsupportedRelocatableCombination {
        alignment: u32,
        pref_address: u64,
    },
    InitramfsTooLarge {
        len: u64,
        limit: u64,
    },
    CmdlineTooLong {
        len: usize,
        limit: usize,
    },
    UnsupportedSetupHeaderField {
        field: &'static str,
        value: u64,
    },
}

impl std::fmt::Display for BzImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BzImageError::TruncatedSetupHeader { len, min } => {
                write!(f, "bzImage setup header truncated: got {len} bytes, need {min}")
            }
            BzImageError::BadHeaderMagic { found } => {
                write!(f, "bzImage setup header magic is not HdrS: {found:02x?}")
            }
            BzImageError::UnsupportedProtocolVersion { found, min } => {
                write!(
                    f,
                    "bzImage protocol {found:#06x} is unsupported, need >= {min:#06x}"
                )
            }
            BzImageError::UnsupportedLoadFlags { loadflags } => {
                write!(
                    f,
                    "bzImage loadflags {loadflags:#04x} do not set LOADED_HIGH"
                )
            }
            BzImageError::UnsupportedXloadflags {
                xloadflags,
                supported,
            } => {
                write!(
                    f,
                    "bzImage xloadflags {xloadflags:#06x} exceed supported mask {supported:#06x}"
                )
            }
            BzImageError::PayloadOverflow => write!(f, "bzImage payload offset overflows"),
            BzImageError::PayloadOutsideImage {
                start,
                len,
                image_len,
            } => write!(
                f,
                "bzImage payload [{start:#x}..+{len:#x}) exceeds image length {image_len:#x}"
            ),
            BzImageError::SetupHeaderTooShort { end, min } => write!(
                f,
                "bzImage setup header copy end {end:#x} is before required parsed end {min:#x}"
            ),
            BzImageError::BadInitSize { init_size } => {
                write!(f, "bzImage init_size {init_size:#x} is not usable")
            }
            BzImageError::BadKernelAlignment { alignment } => write!(
                f,
                "bzImage kernel_alignment {alignment:#x} is not a supported power-of-two alignment"
            ),
            BzImageError::UnsupportedRelocatableCombination {
                alignment,
                pref_address,
            } => write!(
                f,
                "bzImage relocatable kernel has pref_address {pref_address:#x} not aligned to {alignment:#x}"
            ),
            BzImageError::InitramfsTooLarge { len, limit } => {
                write!(f, "initramfs length {len} exceeds placement limit {limit}")
            }
            BzImageError::CmdlineTooLong { len, limit } => {
                write!(f, "Linux cmdline length {len} exceeds limit {limit}")
            }
            BzImageError::UnsupportedSetupHeaderField { field, value } => write!(
                f,
                "bzImage setup header field {field}={value:#x} is outside the deterministic subset"
            ),
        }
    }
}

impl std::error::Error for BzImageError {}

pub fn parse_bzimage(
    image: &[u8],
    initramfs_len: usize,
    cmdline_len: usize,
    mem_bytes: u64,
) -> Result<BzImageLayout, BzImageError> {
    if image.len() < SETUP_HEADER_END {
        return Err(BzImageError::TruncatedSetupHeader {
            len: image.len(),
            min: SETUP_HEADER_END,
        });
    }

    let mut found = [0u8; 4];
    found.copy_from_slice(&image[HEADER_MAGIC_OFF..HEADER_MAGIC_OFF + 4]);
    if &found != HEADER_MAGIC {
        return Err(BzImageError::BadHeaderMagic { found });
    }

    let protocol_version = u16le(image, PROTOCOL_VERSION_OFF);
    if protocol_version < MIN_PROTOCOL_VERSION {
        return Err(BzImageError::UnsupportedProtocolVersion {
            found: protocol_version,
            min: MIN_PROTOCOL_VERSION,
        });
    }

    let setup_header_end = HEADER_MAGIC_OFF + usize::from(image[SETUP_HEADER_LEN_OFF]);
    if setup_header_end < SETUP_HEADER_END {
        return Err(BzImageError::SetupHeaderTooShort {
            end: setup_header_end,
            min: SETUP_HEADER_END,
        });
    }
    let setup_header = image
        .get(SETUP_SECTS_OFF..setup_header_end)
        .ok_or(BzImageError::TruncatedSetupHeader {
            len: image.len(),
            min: setup_header_end,
        })?
        .to_vec();

    let setup_sects_raw = image[SETUP_SECTS_OFF];
    let setup_sects = if setup_sects_raw == 0 {
        DEFAULT_SETUP_SECTS
    } else {
        setup_sects_raw
    };
    let setup_bytes = (u64::from(setup_sects) + 1)
        .checked_mul(SECTOR)
        .ok_or(BzImageError::PayloadOverflow)?;

    let loadflags = image[LOADFLAGS_OFF];
    if loadflags & LOADFLAGS_LOADED_HIGH == 0 {
        return Err(BzImageError::UnsupportedLoadFlags { loadflags });
    }

    let xloadflags = u16le(image, XLOADFLAGS_OFF);
    if xloadflags & XLF_KERNEL_64 == 0 || xloadflags & !XLOADFLAGS_SUPPORTED != 0 {
        return Err(BzImageError::UnsupportedXloadflags {
            xloadflags,
            supported: XLOADFLAGS_SUPPORTED,
        });
    }

    let kernel_alignment = u32le(image, KERNEL_ALIGNMENT_OFF);
    if kernel_alignment < MIN_KERNEL_ALIGNMENT || !kernel_alignment.is_power_of_two() {
        return Err(BzImageError::BadKernelAlignment {
            alignment: kernel_alignment,
        });
    }

    let relocatable_kernel = image[RELOCATABLE_KERNEL_OFF] != 0;
    let pref_address = u64le(image, PREF_ADDRESS_OFF);
    if relocatable_kernel && pref_address % u64::from(kernel_alignment) != 0 {
        return Err(BzImageError::UnsupportedRelocatableCombination {
            alignment: kernel_alignment,
            pref_address,
        });
    }

    let payload_offset = u64::from(u32le(image, PAYLOAD_OFFSET_OFF));
    let payload_length = u64::from(u32le(image, PAYLOAD_LENGTH_OFF));
    let payload_file_offset = setup_bytes
        .checked_add(payload_offset)
        .ok_or(BzImageError::PayloadOverflow)?;
    let payload_end = payload_file_offset
        .checked_add(payload_length)
        .ok_or(BzImageError::PayloadOverflow)?;
    let image_len = image.len() as u64;
    if payload_length == 0 || payload_end > image_len {
        return Err(BzImageError::PayloadOutsideImage {
            start: payload_file_offset,
            len: payload_length,
            image_len,
        });
    }
    let kernel_image_length =
        image_len
            .checked_sub(setup_bytes)
            .ok_or(BzImageError::PayloadOutsideImage {
                start: setup_bytes,
                len: image_len,
                image_len,
            })?;
    if kernel_image_length == 0 {
        return Err(BzImageError::PayloadOutsideImage {
            start: setup_bytes,
            len: 0,
            image_len,
        });
    }

    let init_size = u32le(image, INIT_SIZE_OFF);
    if init_size == 0 {
        return Err(BzImageError::BadInitSize { init_size });
    }

    reject_unsupported_header_field(
        "hardware_subarch",
        u64::from(u32le(image, HARDWARE_SUBARCH_OFF)),
    )?;
    reject_unsupported_header_field(
        "hardware_subarch_data",
        u64le(image, HARDWARE_SUBARCH_DATA_OFF),
    )?;
    reject_unsupported_header_field("setup_data", u64le(image, SETUP_DATA_OFF))?;

    let cmdline_size = u32le(image, CMDLINE_SIZE_OFF);
    let cmdline_limit = usize::try_from(cmdline_size)
        .unwrap_or(usize::MAX)
        .min(config::MAX_CMDLINE);
    if cmdline_len > cmdline_limit {
        return Err(BzImageError::CmdlineTooLong {
            len: cmdline_len,
            limit: cmdline_limit,
        });
    }

    let initrd_addr_max = u32le(image, INITRD_ADDR_MAX_OFF);
    let initrd_addr_limit = u64::from(initrd_addr_max).saturating_add(1);
    let placement_limit = mem_bytes.min(initrd_addr_limit);
    let initramfs_len = initramfs_len as u64;
    if initramfs_len > placement_limit {
        return Err(BzImageError::InitramfsTooLarge {
            len: initramfs_len,
            limit: placement_limit,
        });
    }

    Ok(BzImageLayout {
        setup_header,
        protocol_version,
        setup_sects,
        setup_bytes,
        kernel_image_file_offset: setup_bytes,
        kernel_image_length,
        payload_file_offset,
        payload_length,
        init_size: u64::from(init_size),
        loadflags,
        xloadflags,
        kernel_alignment,
        relocatable_kernel,
        pref_address,
        initrd_addr_max,
        cmdline_size,
    })
}

fn reject_unsupported_header_field(field: &'static str, value: u64) -> Result<(), BzImageError> {
    if value == 0 {
        Ok(())
    } else {
        Err(BzImageError::UnsupportedSetupHeaderField { field, value })
    }
}

fn u16le(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}

fn u32le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn u64le(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinuxMemoryRange {
    pub start: u64,
    pub len: u64,
}

impl LinuxMemoryRange {
    pub const fn new(start: u64, len: u64) -> Self {
        Self { start, len }
    }

    pub const fn end(&self) -> u64 {
        self.start + self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub const fn overlaps(&self, other: &Self) -> bool {
        self.start < other.end() && other.start < self.end()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct E820Entry {
    pub addr: u64,
    pub size: u64,
    pub kind: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinuxBootLayout {
    pub page_tables: LinuxMemoryRange,
    pub boot_params: LinuxMemoryRange,
    pub cmdline: LinuxMemoryRange,
    pub cmdline_len: u32,
    pub kernel_image: LinuxMemoryRange,
    pub kernel_image_file_offset: u64,
    pub compressed_payload_file_offset: u64,
    pub compressed_payload_length: u64,
    pub initramfs: Option<LinuxMemoryRange>,
    pub device_mmio: LinuxMemoryRange,
    pub apic_mmio: LinuxMemoryRange,
    pub e820_entries: Vec<E820Entry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinuxBootPlan {
    pub layout: LinuxBootLayout,
    pub boot_params: [u8; LINUX_BOOT_PARAMS_SIZE],
    /// Command-line bytes written at `layout.cmdline.start`, including the
    /// Linux-required trailing NUL byte.
    pub cmdline_image: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinuxBootLayoutError {
    CmdlineTooLong {
        len: usize,
        limit: usize,
    },
    MemOverlapsApicMmio {
        mem_bytes: u64,
        apic_base: u64,
    },
    MemOverlapsDeviceWindow {
        mem_bytes: u64,
        device_base: u64,
    },
    RangeOverflow {
        label: &'static str,
    },
    KernelOutsideRam {
        end: u64,
        mem_bytes: u64,
    },
    InitramfsTooLarge {
        len: u64,
        limit: u64,
    },
    InitramfsOverlapsKernel {
        start: u64,
        kernel_reserved_end: u64,
    },
    E820TooManyEntries {
        count: usize,
        max: usize,
    },
}

impl std::fmt::Display for LinuxBootLayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinuxBootLayoutError::CmdlineTooLong { len, limit } => {
                write!(f, "Linux cmdline length {len} exceeds layout limit {limit}")
            }
            LinuxBootLayoutError::MemOverlapsApicMmio {
                mem_bytes,
                apic_base,
            } => write!(
                f,
                "guest RAM end {mem_bytes:#x} overlaps APIC MMIO base {apic_base:#x}"
            ),
            LinuxBootLayoutError::MemOverlapsDeviceWindow {
                mem_bytes,
                device_base,
            } => write!(
                f,
                "guest RAM end {mem_bytes:#x} overlaps deterministic device window {device_base:#x}"
            ),
            LinuxBootLayoutError::RangeOverflow { label } => {
                write!(f, "{label} placement overflowed u64")
            }
            LinuxBootLayoutError::KernelOutsideRam { end, mem_bytes } => write!(
                f,
                "bzImage kernel image end {end:#x} exceeds guest RAM {mem_bytes:#x}"
            ),
            LinuxBootLayoutError::InitramfsTooLarge { len, limit } => {
                write!(f, "initramfs length {len} exceeds placement limit {limit}")
            }
            LinuxBootLayoutError::InitramfsOverlapsKernel {
                start,
                kernel_reserved_end,
            } => write!(
                f,
                "initramfs start {start:#x} overlaps kernel reservation ending at {kernel_reserved_end:#x}"
            ),
            LinuxBootLayoutError::E820TooManyEntries { count, max } => {
                write!(f, "e820 entry count {count} exceeds boot_params cap {max}")
            }
        }
    }
}

impl std::error::Error for LinuxBootLayoutError {}

pub fn plan_bzimage_boot(
    header: &BzImageLayout,
    mem_bytes: u64,
    initramfs_len: usize,
    cmdline: &[u8],
) -> Result<LinuxBootPlan, LinuxBootLayoutError> {
    if cmdline.len() > config::MAX_CMDLINE {
        return Err(LinuxBootLayoutError::CmdlineTooLong {
            len: cmdline.len(),
            limit: config::MAX_CMDLINE,
        });
    }
    let cmdline_image_len = u64::try_from(cmdline.len() + 1)
        .map_err(|_| LinuxBootLayoutError::RangeOverflow { label: "cmdline" })?;
    if cmdline_image_len > LINUX_CMDLINE_RESERVED_LEN {
        return Err(LinuxBootLayoutError::CmdlineTooLong {
            len: cmdline.len(),
            limit: (LINUX_CMDLINE_RESERVED_LEN - 1) as usize,
        });
    }
    if mem_bytes > LINUX_APIC_MMIO_BASE {
        return Err(LinuxBootLayoutError::MemOverlapsApicMmio {
            mem_bytes,
            apic_base: LINUX_APIC_MMIO_BASE,
        });
    }
    if mem_bytes > MMIO_HOLE_BASE {
        return Err(LinuxBootLayoutError::MemOverlapsDeviceWindow {
            mem_bytes,
            device_base: MMIO_HOLE_BASE,
        });
    }

    let kernel_reserved_len =
        align_up(header.kernel_image_length.max(header.init_size), PAGE_SIZE)?;
    let kernel_reserved_end = LINUX_KERNEL_LOAD_GPA
        .checked_add(kernel_reserved_len)
        .ok_or(LinuxBootLayoutError::RangeOverflow { label: "kernel" })?;
    if kernel_reserved_end > mem_bytes {
        return Err(LinuxBootLayoutError::KernelOutsideRam {
            end: kernel_reserved_end,
            mem_bytes,
        });
    }

    let initramfs_len = initramfs_len as u64;
    let initramfs_limit = mem_bytes.min(u64::from(header.initrd_addr_max).saturating_add(1));
    if initramfs_len > initramfs_limit {
        return Err(LinuxBootLayoutError::InitramfsTooLarge {
            len: initramfs_len,
            limit: initramfs_limit,
        });
    }
    let initramfs = if initramfs_len == 0 {
        None
    } else {
        let initramfs_reserved_len = align_up(initramfs_len, PAGE_SIZE)?;
        let initramfs_end = align_down(initramfs_limit, PAGE_SIZE);
        let initramfs_start = initramfs_end.checked_sub(initramfs_reserved_len).ok_or(
            LinuxBootLayoutError::InitramfsTooLarge {
                len: initramfs_len,
                limit: initramfs_limit,
            },
        )?;
        if initramfs_start < kernel_reserved_end {
            return Err(LinuxBootLayoutError::InitramfsOverlapsKernel {
                start: initramfs_start,
                kernel_reserved_end,
            });
        }
        Some(LinuxMemoryRange::new(initramfs_start, initramfs_len))
    };

    let page_tables = LinuxMemoryRange::new(LINUX_PAGE_TABLES_GPA, LINUX_PAGE_TABLES_LEN);
    let boot_params = LinuxMemoryRange::new(LINUX_BOOT_PARAMS_GPA, LINUX_BOOT_PARAMS_SIZE as u64);
    let cmdline_range = LinuxMemoryRange::new(LINUX_CMDLINE_GPA, LINUX_CMDLINE_RESERVED_LEN);
    let kernel_image = LinuxMemoryRange::new(LINUX_KERNEL_LOAD_GPA, header.kernel_image_length);
    let device_mmio = LinuxMemoryRange::new(MMIO_HOLE_BASE, MMIO_HOLE_LEN);
    let apic_mmio = LinuxMemoryRange::new(LINUX_APIC_MMIO_BASE, LINUX_APIC_MMIO_LEN);

    let e820_entries = build_e820_entries(mem_bytes, kernel_reserved_len, initramfs)?;
    let layout = LinuxBootLayout {
        page_tables,
        boot_params,
        cmdline: cmdline_range,
        cmdline_len: cmdline.len() as u32,
        kernel_image,
        kernel_image_file_offset: header.kernel_image_file_offset,
        compressed_payload_file_offset: header.payload_file_offset,
        compressed_payload_length: header.payload_length,
        initramfs,
        device_mmio,
        apic_mmio,
        e820_entries,
    };

    let boot_params = build_boot_params(header, &layout)?;
    let mut cmdline_image = Vec::with_capacity(cmdline.len() + 1);
    cmdline_image.extend_from_slice(cmdline);
    cmdline_image.push(0);

    Ok(LinuxBootPlan {
        layout,
        boot_params,
        cmdline_image,
    })
}

fn build_boot_params(
    header: &BzImageLayout,
    layout: &LinuxBootLayout,
) -> Result<[u8; LINUX_BOOT_PARAMS_SIZE], LinuxBootLayoutError> {
    if layout.e820_entries.len() > E820_TABLE_CAP {
        return Err(LinuxBootLayoutError::E820TooManyEntries {
            count: layout.e820_entries.len(),
            max: E820_TABLE_CAP,
        });
    }

    let mut page = [0u8; LINUX_BOOT_PARAMS_SIZE];
    let setup_header_end = SETUP_SECTS_OFF + header.setup_header.len();
    if setup_header_end > page.len() {
        return Err(LinuxBootLayoutError::RangeOverflow {
            label: "setup_header",
        });
    }
    page[SETUP_SECTS_OFF..setup_header_end].copy_from_slice(&header.setup_header);

    page[ZERO_PAGE_E820_COUNT_OFF] = layout.e820_entries.len() as u8;
    put_u8(&mut page, SETUP_SECTS_OFF, header.setup_sects);
    page[HEADER_MAGIC_OFF..HEADER_MAGIC_OFF + 4].copy_from_slice(HEADER_MAGIC);
    put_u16(&mut page, PROTOCOL_VERSION_OFF, header.protocol_version);
    put_u8(&mut page, TYPE_OF_LOADER_OFF, LINUX_TYPE_OF_LOADER);
    put_u8(
        &mut page,
        LOADFLAGS_OFF,
        header.loadflags | LOADFLAGS_CAN_USE_HEAP,
    );
    put_u32(
        &mut page,
        CODE32_START_OFF,
        layout.kernel_image.start as u32,
    );
    if let Some(initramfs) = layout.initramfs {
        put_u32(&mut page, RAMDISK_IMAGE_OFF, initramfs.start as u32);
        put_u32(&mut page, RAMDISK_SIZE_OFF, initramfs.len as u32);
    }
    put_u16(&mut page, HEAP_END_PTR_OFF, LINUX_HEAP_END_PTR);
    put_u32(&mut page, CMD_LINE_PTR_OFF, layout.cmdline.start as u32);
    put_u32(&mut page, INITRD_ADDR_MAX_OFF, header.initrd_addr_max);
    put_u32(&mut page, KERNEL_ALIGNMENT_OFF, header.kernel_alignment);
    put_u8(
        &mut page,
        RELOCATABLE_KERNEL_OFF,
        u8::from(header.relocatable_kernel),
    );
    put_u16(&mut page, XLOADFLAGS_OFF, header.xloadflags);
    put_u32(&mut page, CMDLINE_SIZE_OFF, header.cmdline_size);
    put_u64(&mut page, PREF_ADDRESS_OFF, header.pref_address);

    for (index, entry) in layout.e820_entries.iter().enumerate() {
        let at = ZERO_PAGE_E820_TABLE_OFF + index * E820_ENTRY_SIZE;
        put_u64(&mut page, at, entry.addr);
        put_u64(&mut page, at + 8, entry.size);
        put_u32(&mut page, at + 16, entry.kind);
    }

    Ok(page)
}

fn build_e820_entries(
    mem_bytes: u64,
    kernel_reserved_len: u64,
    initramfs: Option<LinuxMemoryRange>,
) -> Result<Vec<E820Entry>, LinuxBootLayoutError> {
    let mut reserved = vec![
        LinuxMemoryRange::new(0, LINUX_LOW_RESERVED_LEN),
        LinuxMemoryRange::new(LINUX_LEGACY_IO_HOLE_BASE, LINUX_LEGACY_IO_HOLE_LEN),
        LinuxMemoryRange::new(LINUX_KERNEL_LOAD_GPA, kernel_reserved_len),
    ];
    if let Some(initramfs) = initramfs {
        reserved.push(LinuxMemoryRange::new(
            initramfs.start,
            align_up(initramfs.len, PAGE_SIZE)?,
        ));
    }
    reserved.sort_by_key(|range| range.start);

    let mut entries = Vec::with_capacity(reserved.len() * 2 + 2);
    let mut cursor = 0;
    for range in reserved.into_iter().filter(|range| range.start < mem_bytes) {
        let end = range.end().min(mem_bytes);
        if cursor < range.start {
            push_e820(&mut entries, cursor, range.start - cursor, E820_RAM);
        }
        if cursor < end {
            push_e820(&mut entries, range.start, end - range.start, E820_RESERVED);
            cursor = end;
        }
    }
    if cursor < mem_bytes {
        push_e820(&mut entries, cursor, mem_bytes - cursor, E820_RAM);
    }
    push_e820(&mut entries, MMIO_HOLE_BASE, MMIO_HOLE_LEN, E820_RESERVED);
    push_e820(
        &mut entries,
        LINUX_APIC_MMIO_BASE,
        LINUX_APIC_MMIO_LEN,
        E820_RESERVED,
    );

    if entries.len() > E820_TABLE_CAP {
        return Err(LinuxBootLayoutError::E820TooManyEntries {
            count: entries.len(),
            max: E820_TABLE_CAP,
        });
    }
    Ok(entries)
}

fn push_e820(entries: &mut Vec<E820Entry>, addr: u64, size: u64, kind: u32) {
    if size != 0 {
        entries.push(E820Entry { addr, size, kind });
    }
}

fn align_up(value: u64, align: u64) -> Result<u64, LinuxBootLayoutError> {
    let mask = align - 1;
    value
        .checked_add(mask)
        .map(|v| v & !mask)
        .ok_or(LinuxBootLayoutError::RangeOverflow { label: "align_up" })
}

fn align_down(value: u64, align: u64) -> u64 {
    value & !(align - 1)
}

fn put_u8(page: &mut [u8], at: usize, value: u8) {
    page[at] = value;
}

fn put_u16(page: &mut [u8], at: usize, value: u16) {
    page[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(page: &mut [u8], at: usize, value: u32) {
    page[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(page: &mut [u8], at: usize, value: u64) {
    page[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEM: u64 = 64 * 1024 * 1024;
    const INITRAMFS_LEN: usize = 4096;
    const CMDLINE_LEN: usize = 128;

    fn synthetic_bzimage() -> Vec<u8> {
        let setup_sects = 4u8;
        let setup_bytes = (u64::from(setup_sects) + 1) * SECTOR;
        let payload_offset = 0x400u32;
        let payload_length = 0x800u32;
        let init_size = 0x40_0000u32;
        let total = setup_bytes as usize + payload_offset as usize + payload_length as usize;
        let mut image = vec![0u8; total];
        image[SETUP_SECTS_OFF] = setup_sects;
        image[SETUP_HEADER_LEN_OFF] = (SETUP_HEADER_END - HEADER_MAGIC_OFF) as u8;
        image[0x1fe..0x200].copy_from_slice(&0xaa55u16.to_le_bytes());
        image[0x200..0x202].copy_from_slice(&[0xeb, 0x66]);
        image[HEADER_MAGIC_OFF..HEADER_MAGIC_OFF + 4].copy_from_slice(HEADER_MAGIC);
        image[PROTOCOL_VERSION_OFF..PROTOCOL_VERSION_OFF + 2]
            .copy_from_slice(&MIN_PROTOCOL_VERSION.to_le_bytes());
        image[LOADFLAGS_OFF] = LOADFLAGS_LOADED_HIGH;
        image[INITRD_ADDR_MAX_OFF..INITRD_ADDR_MAX_OFF + 4]
            .copy_from_slice(&0x37ff_ffffu32.to_le_bytes());
        image[KERNEL_ALIGNMENT_OFF..KERNEL_ALIGNMENT_OFF + 4]
            .copy_from_slice(&MIN_KERNEL_ALIGNMENT.to_le_bytes());
        image[RELOCATABLE_KERNEL_OFF] = 1;
        image[XLOADFLAGS_OFF..XLOADFLAGS_OFF + 2].copy_from_slice(&XLF_KERNEL_64.to_le_bytes());
        image[CMDLINE_SIZE_OFF..CMDLINE_SIZE_OFF + 4]
            .copy_from_slice(&(config::MAX_CMDLINE as u32).to_le_bytes());
        image[PAYLOAD_OFFSET_OFF..PAYLOAD_OFFSET_OFF + 4]
            .copy_from_slice(&payload_offset.to_le_bytes());
        image[PAYLOAD_LENGTH_OFF..PAYLOAD_LENGTH_OFF + 4]
            .copy_from_slice(&payload_length.to_le_bytes());
        image[PREF_ADDRESS_OFF..PREF_ADDRESS_OFF + 8].copy_from_slice(&0x20_0000u64.to_le_bytes());
        image[INIT_SIZE_OFF..INIT_SIZE_OFF + 4].copy_from_slice(&init_size.to_le_bytes());
        let payload_start = setup_bytes as usize + payload_offset as usize;
        image[setup_bytes as usize..payload_start].fill(0x5a);
        image[setup_bytes as usize + 0x200] = 0xcc;
        image[payload_start..payload_start + payload_length as usize].fill(0xa5);
        image
    }

    #[test]
    fn linux_bzimage_valid_synthetic_image_parses() {
        let image = synthetic_bzimage();
        let layout = parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM).unwrap();
        assert_eq!(layout.protocol_version, MIN_PROTOCOL_VERSION);
        assert_eq!(layout.setup_sects, 4);
        assert_eq!(layout.setup_bytes, 5 * SECTOR);
        assert_eq!(layout.kernel_image_file_offset, 5 * SECTOR);
        assert_eq!(layout.kernel_image_length, 0xc00);
        assert_eq!(layout.payload_file_offset, 5 * SECTOR + 0x400);
        assert_eq!(layout.payload_length, 0x800);
        assert_eq!(layout.init_size, 0x40_0000);
        assert_eq!(layout.kernel_alignment, MIN_KERNEL_ALIGNMENT);
        assert!(layout.relocatable_kernel);
        assert_eq!(layout.initrd_addr_max, 0x37ff_ffff);
    }

    #[test]
    fn linux_bzimage_rejects_bad_magic() {
        let mut image = synthetic_bzimage();
        image[HEADER_MAGIC_OFF..HEADER_MAGIC_OFF + 4].copy_from_slice(b"Bad!");
        assert_eq!(
            parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM),
            Err(BzImageError::BadHeaderMagic { found: *b"Bad!" })
        );
    }

    #[test]
    fn linux_bzimage_rejects_truncated_setup_header() {
        let image = vec![0u8; SETUP_HEADER_END - 1];
        assert_eq!(
            parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM),
            Err(BzImageError::TruncatedSetupHeader {
                len: SETUP_HEADER_END - 1,
                min: SETUP_HEADER_END
            })
        );
    }

    #[test]
    fn linux_bzimage_rejects_unsupported_protocol_version() {
        let mut image = synthetic_bzimage();
        image[PROTOCOL_VERSION_OFF..PROTOCOL_VERSION_OFF + 2]
            .copy_from_slice(&0x0209u16.to_le_bytes());
        assert_eq!(
            parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM),
            Err(BzImageError::UnsupportedProtocolVersion {
                found: 0x0209,
                min: MIN_PROTOCOL_VERSION
            })
        );
    }

    #[test]
    fn linux_bzimage_rejects_missing_loaded_high_flag() {
        let mut image = synthetic_bzimage();
        image[LOADFLAGS_OFF] = 0;
        assert_eq!(
            parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM),
            Err(BzImageError::UnsupportedLoadFlags { loadflags: 0 })
        );
    }

    #[test]
    fn linux_bzimage_rejects_payload_overflow() {
        let mut image = synthetic_bzimage();
        image[PAYLOAD_OFFSET_OFF..PAYLOAD_OFFSET_OFF + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM),
            Err(BzImageError::PayloadOutsideImage { .. })
        ));

        let mut image = synthetic_bzimage();
        image[PAYLOAD_LENGTH_OFF..PAYLOAD_LENGTH_OFF + 4].copy_from_slice(&0u32.to_le_bytes());
        assert!(matches!(
            parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM),
            Err(BzImageError::PayloadOutsideImage { .. })
        ));
    }

    #[test]
    fn linux_bzimage_rejects_short_setup_header_copy_range() {
        let mut image = synthetic_bzimage();
        image[SETUP_HEADER_LEN_OFF] = 0x20;
        assert_eq!(
            parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM),
            Err(BzImageError::SetupHeaderTooShort {
                end: HEADER_MAGIC_OFF + 0x20,
                min: SETUP_HEADER_END
            })
        );
    }

    #[test]
    fn linux_bzimage_rejects_zero_init_size() {
        let mut image = synthetic_bzimage();
        image[INIT_SIZE_OFF..INIT_SIZE_OFF + 4].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM),
            Err(BzImageError::BadInitSize { init_size: 0 })
        );
    }

    #[test]
    fn linux_bzimage_rejects_unsupported_xloadflags() {
        let mut image = synthetic_bzimage();
        image[XLOADFLAGS_OFF..XLOADFLAGS_OFF + 2]
            .copy_from_slice(&(XLF_KERNEL_64 | XLF_MEM_ENCRYPTION).to_le_bytes());
        assert_eq!(
            parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM),
            Err(BzImageError::UnsupportedXloadflags {
                xloadflags: XLF_KERNEL_64 | XLF_MEM_ENCRYPTION,
                supported: XLOADFLAGS_SUPPORTED
            })
        );
    }

    #[test]
    fn linux_bzimage_accepts_known_non_entry_xloadflags() {
        let mut image = synthetic_bzimage();
        image[XLOADFLAGS_OFF..XLOADFLAGS_OFF + 2]
            .copy_from_slice(&(XLF_KERNEL_64 | XLF_EFI_HANDOVER_64).to_le_bytes());
        let layout = parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM).unwrap();
        assert_eq!(layout.xloadflags, XLF_KERNEL_64 | XLF_EFI_HANDOVER_64);
    }

    #[test]
    fn linux_bzimage_accepts_known_5level_xloadflags() {
        let mut image = synthetic_bzimage();
        image[XLOADFLAGS_OFF..XLOADFLAGS_OFF + 2]
            .copy_from_slice(&(XLF_KERNEL_64 | XLF_5LEVEL | XLF_5LEVEL_ENABLED).to_le_bytes());
        let layout = parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM).unwrap();
        assert_eq!(
            layout.xloadflags,
            XLF_KERNEL_64 | XLF_5LEVEL | XLF_5LEVEL_ENABLED
        );
    }

    #[test]
    fn linux_bzimage_rejects_bad_kernel_alignment() {
        let mut image = synthetic_bzimage();
        image[KERNEL_ALIGNMENT_OFF..KERNEL_ALIGNMENT_OFF + 4]
            .copy_from_slice(&0x30_0000u32.to_le_bytes());
        assert_eq!(
            parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM),
            Err(BzImageError::BadKernelAlignment {
                alignment: 0x30_0000
            })
        );
    }

    #[test]
    fn linux_bzimage_rejects_unsupported_relocatable_feature_combination() {
        let mut image = synthetic_bzimage();
        image[PREF_ADDRESS_OFF..PREF_ADDRESS_OFF + 8].copy_from_slice(&0x30_0000u64.to_le_bytes());
        assert_eq!(
            parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM),
            Err(BzImageError::UnsupportedRelocatableCombination {
                alignment: MIN_KERNEL_ALIGNMENT,
                pref_address: 0x30_0000
            })
        );
    }

    #[test]
    fn linux_bzimage_rejects_initramfs_too_large_for_placement() {
        let image = synthetic_bzimage();
        assert_eq!(
            parse_bzimage(&image, (MEM + 1) as usize, CMDLINE_LEN, MEM),
            Err(BzImageError::InitramfsTooLarge {
                len: MEM + 1,
                limit: MEM
            })
        );
    }

    #[test]
    fn linux_bzimage_rejects_cmdline_too_long() {
        let mut image = synthetic_bzimage();
        image[CMDLINE_SIZE_OFF..CMDLINE_SIZE_OFF + 4].copy_from_slice(&16u32.to_le_bytes());
        assert_eq!(
            parse_bzimage(&image, INITRAMFS_LEN, 17, MEM),
            Err(BzImageError::CmdlineTooLong { len: 17, limit: 16 })
        );
    }

    #[test]
    fn linux_bzimage_rejects_unsupported_setup_header_pointers() {
        let mut image = synthetic_bzimage();
        image[SETUP_DATA_OFF..SETUP_DATA_OFF + 8].copy_from_slice(&0xfeed_cafeu64.to_le_bytes());
        assert_eq!(
            parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM),
            Err(BzImageError::UnsupportedSetupHeaderField {
                field: "setup_data",
                value: 0xfeed_cafe,
            })
        );

        let mut image = synthetic_bzimage();
        image[HARDWARE_SUBARCH_OFF..HARDWARE_SUBARCH_OFF + 4].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM),
            Err(BzImageError::UnsupportedSetupHeaderField {
                field: "hardware_subarch",
                value: 1,
            })
        );

        let mut image = synthetic_bzimage();
        image[HARDWARE_SUBARCH_DATA_OFF..HARDWARE_SUBARCH_DATA_OFF + 8]
            .copy_from_slice(&0x1234u64.to_le_bytes());
        assert_eq!(
            parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM),
            Err(BzImageError::UnsupportedSetupHeaderField {
                field: "hardware_subarch_data",
                value: 0x1234,
            })
        );
    }

    fn parsed_header() -> BzImageLayout {
        let image = synthetic_bzimage();
        parse_bzimage(&image, INITRAMFS_LEN, CMDLINE_LEN, MEM).unwrap()
    }

    fn u16_at(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn u64_at(bytes: &[u8], at: usize) -> u64 {
        u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
    }

    #[test]
    fn linux_boot_layout_pins_gpas_and_non_overlapping_ranges() {
        let header = parsed_header();
        let plan = plan_bzimage_boot(&header, MEM, INITRAMFS_LEN, b"quiet").unwrap();
        let initramfs = plan.layout.initramfs.unwrap();

        assert_eq!(
            plan.layout.page_tables,
            LinuxMemoryRange::new(0x1000, 0x6000)
        );
        assert_eq!(
            plan.layout.boot_params,
            LinuxMemoryRange::new(LINUX_BOOT_PARAMS_GPA, 0x1000)
        );
        assert_eq!(
            plan.layout.cmdline,
            LinuxMemoryRange::new(LINUX_CMDLINE_GPA, 0x2000)
        );
        assert_eq!(
            plan.layout.kernel_image,
            LinuxMemoryRange::new(LINUX_KERNEL_LOAD_GPA, 0xc00)
        );
        assert_eq!(plan.layout.kernel_image_file_offset, 5 * SECTOR);
        assert_eq!(
            plan.layout.compressed_payload_file_offset,
            5 * SECTOR + 0x400
        );
        assert_eq!(plan.layout.compressed_payload_length, 0x800);
        assert_eq!(
            initramfs,
            LinuxMemoryRange::new(MEM - PAGE_SIZE, INITRAMFS_LEN as u64)
        );
        assert_eq!(
            plan.layout.device_mmio,
            LinuxMemoryRange::new(MMIO_HOLE_BASE, MMIO_HOLE_LEN)
        );
        assert_eq!(
            plan.layout.apic_mmio,
            LinuxMemoryRange::new(LINUX_APIC_MMIO_BASE, LINUX_APIC_MMIO_LEN)
        );

        let ranges = [
            plan.layout.page_tables,
            plan.layout.boot_params,
            plan.layout.cmdline,
            plan.layout.kernel_image,
            initramfs,
        ];
        for (index, left) in ranges.iter().enumerate() {
            for right in ranges.iter().skip(index + 1) {
                assert!(!left.overlaps(right), "range {left:x?} overlaps {right:x?}");
            }
        }
        for range in ranges {
            assert!(range.end() <= MEM);
            assert!(!range.overlaps(&plan.layout.device_mmio));
            assert!(!range.overlaps(&plan.layout.apic_mmio));
        }
    }

    #[test]
    fn linux_boot_params_encode_e820_cmdline_initramfs_and_kernel() {
        let header = parsed_header();
        let plan = plan_bzimage_boot(&header, MEM, INITRAMFS_LEN, b"quiet").unwrap();
        let initramfs = plan.layout.initramfs.unwrap();
        let kernel_reserved_len = align_up(header.init_size, PAGE_SIZE).unwrap();
        let kernel_reserved_end = LINUX_KERNEL_LOAD_GPA + kernel_reserved_len;

        assert_eq!(
            plan.layout.e820_entries,
            vec![
                E820Entry {
                    addr: 0,
                    size: LINUX_LOW_RESERVED_LEN,
                    kind: E820_RESERVED,
                },
                E820Entry {
                    addr: LINUX_LOW_RESERVED_LEN,
                    size: LINUX_LEGACY_IO_HOLE_BASE - LINUX_LOW_RESERVED_LEN,
                    kind: E820_RAM,
                },
                E820Entry {
                    addr: LINUX_LEGACY_IO_HOLE_BASE,
                    size: LINUX_LEGACY_IO_HOLE_LEN,
                    kind: E820_RESERVED,
                },
                E820Entry {
                    addr: LINUX_KERNEL_LOAD_GPA,
                    size: kernel_reserved_len,
                    kind: E820_RESERVED,
                },
                E820Entry {
                    addr: kernel_reserved_end,
                    size: initramfs.start - kernel_reserved_end,
                    kind: E820_RAM,
                },
                E820Entry {
                    addr: initramfs.start,
                    size: PAGE_SIZE,
                    kind: E820_RESERVED,
                },
                E820Entry {
                    addr: MMIO_HOLE_BASE,
                    size: MMIO_HOLE_LEN,
                    kind: E820_RESERVED,
                },
                E820Entry {
                    addr: LINUX_APIC_MMIO_BASE,
                    size: LINUX_APIC_MMIO_LEN,
                    kind: E820_RESERVED,
                },
            ]
        );

        let page = &plan.boot_params;
        assert_eq!(page[ZERO_PAGE_E820_COUNT_OFF], 8);
        assert_eq!(page[SETUP_SECTS_OFF], 4);
        assert_eq!(u16_at(page, 0x1fe), 0xaa55);
        assert_eq!(&page[0x200..0x202], &[0xeb, 0x66]);
        assert_eq!(&page[HEADER_MAGIC_OFF..HEADER_MAGIC_OFF + 4], b"HdrS");
        assert_eq!(u16_at(page, PROTOCOL_VERSION_OFF), MIN_PROTOCOL_VERSION);
        assert_eq!(page[TYPE_OF_LOADER_OFF], LINUX_TYPE_OF_LOADER);
        assert_eq!(
            page[LOADFLAGS_OFF],
            LOADFLAGS_LOADED_HIGH | LOADFLAGS_CAN_USE_HEAP
        );
        assert_eq!(u32_at(page, CODE32_START_OFF), LINUX_KERNEL_LOAD_GPA as u32);
        assert_eq!(u32_at(page, RAMDISK_IMAGE_OFF), initramfs.start as u32);
        assert_eq!(u32_at(page, RAMDISK_SIZE_OFF), INITRAMFS_LEN as u32);
        assert_eq!(u16_at(page, HEAP_END_PTR_OFF), LINUX_HEAP_END_PTR);
        assert_eq!(u32_at(page, CMD_LINE_PTR_OFF), LINUX_CMDLINE_GPA as u32);
        assert_eq!(u32_at(page, INITRD_ADDR_MAX_OFF), header.initrd_addr_max);
        assert_eq!(u32_at(page, KERNEL_ALIGNMENT_OFF), MIN_KERNEL_ALIGNMENT);
        assert_eq!(page[RELOCATABLE_KERNEL_OFF], 1);
        assert_eq!(u16_at(page, XLOADFLAGS_OFF), XLF_KERNEL_64);
        assert_eq!(u32_at(page, CMDLINE_SIZE_OFF), config::MAX_CMDLINE as u32);
        assert_eq!(u64_at(page, PREF_ADDRESS_OFF), 0x20_0000);
        assert_eq!(u32_at(page, INIT_SIZE_OFF), header.init_size as u32);
        assert_eq!(plan.layout.cmdline_len, 5);
        assert_eq!(plan.cmdline_image, b"quiet\0");

        let first_e820 = ZERO_PAGE_E820_TABLE_OFF;
        assert_eq!(u64_at(page, first_e820), 0);
        assert_eq!(u64_at(page, first_e820 + 8), LINUX_LOW_RESERVED_LEN);
        assert_eq!(u32_at(page, first_e820 + 16), E820_RESERVED);
    }

    #[test]
    fn linux_boot_params_are_zero_filled_outside_contract_fields() {
        let header = parsed_header();
        let plan = plan_bzimage_boot(&header, MEM, INITRAMFS_LEN, b"quiet").unwrap();
        let mut scrubbed = plan.boot_params;
        let setup_header_end = SETUP_SECTS_OFF + header.setup_header.len();
        let e820_end = ZERO_PAGE_E820_TABLE_OFF + plan.layout.e820_entries.len() * E820_ENTRY_SIZE;
        for range in [
            ZERO_PAGE_E820_COUNT_OFF..ZERO_PAGE_E820_COUNT_OFF + 1,
            SETUP_SECTS_OFF..setup_header_end,
            ZERO_PAGE_E820_TABLE_OFF..e820_end,
        ] {
            scrubbed[range].fill(0);
        }
        assert_eq!(scrubbed, [0u8; LINUX_BOOT_PARAMS_SIZE]);
    }

    #[test]
    fn linux_boot_layout_is_stable_byte_for_byte() {
        let header = parsed_header();
        let a = plan_bzimage_boot(&header, MEM, INITRAMFS_LEN, b"quiet").unwrap();
        let b = plan_bzimage_boot(&header, MEM, INITRAMFS_LEN, b"quiet").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn linux_boot_layout_rejects_overlap_configurations() {
        let header = parsed_header();
        assert_eq!(
            plan_bzimage_boot(&header, MMIO_HOLE_BASE + PAGE_SIZE, INITRAMFS_LEN, b"quiet"),
            Err(LinuxBootLayoutError::MemOverlapsDeviceWindow {
                mem_bytes: MMIO_HOLE_BASE + PAGE_SIZE,
                device_base: MMIO_HOLE_BASE,
            })
        );
        assert_eq!(
            plan_bzimage_boot(
                &header,
                LINUX_APIC_MMIO_BASE + PAGE_SIZE,
                INITRAMFS_LEN,
                b"quiet"
            ),
            Err(LinuxBootLayoutError::MemOverlapsApicMmio {
                mem_bytes: LINUX_APIC_MMIO_BASE + PAGE_SIZE,
                apic_base: LINUX_APIC_MMIO_BASE,
            })
        );
        assert_eq!(
            plan_bzimage_boot(&header, LINUX_KERNEL_LOAD_GPA, INITRAMFS_LEN, b"quiet"),
            Err(LinuxBootLayoutError::KernelOutsideRam {
                end: LINUX_KERNEL_LOAD_GPA + header.init_size,
                mem_bytes: LINUX_KERNEL_LOAD_GPA,
            })
        );
        assert!(matches!(
            plan_bzimage_boot(
                &header,
                MEM,
                INITRAMFS_LEN,
                &vec![b'x'; config::MAX_CMDLINE + 1]
            ),
            Err(LinuxBootLayoutError::CmdlineTooLong { .. })
        ));

        let mut low_initrd_limit = header.clone();
        low_initrd_limit.initrd_addr_max = 0x001f_ffff;
        assert_eq!(
            plan_bzimage_boot(&low_initrd_limit, MEM, 0x10_0000, b"quiet"),
            Err(LinuxBootLayoutError::InitramfsOverlapsKernel {
                start: LINUX_KERNEL_LOAD_GPA,
                kernel_reserved_end: LINUX_KERNEL_LOAD_GPA + header.init_size,
            })
        );
    }
}
