//! CreateVm image-cache resolver (bead p8g).
//!
//! Public CreateVm carries content hashes, while the boot and pv-blk paths
//! consume deterministic bytes/files. This module is the daemon-owned seam
//! between those contracts: a flat local cache keyed by lowercase BLAKE3 hex,
//! with every opened blob verified before it reaches KVM/device setup.

use dh_vmm::blkfile::FileBase;
use dh_vmm::config::{BootSpec, ConfigError, MachineConfig};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub const DEFAULT_IMAGE_CACHE_DIR: &str = "/var/lib/dh/images";
pub const MAX_BASE_IMAGE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_KERNEL_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_INITRAMFS_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageBlobKind {
    BaseImage,
    Kernel,
    Initramfs,
}

impl ImageBlobKind {
    fn label(self) -> &'static str {
        match self {
            ImageBlobKind::BaseImage => "base image",
            ImageBlobKind::Kernel => "kernel",
            ImageBlobKind::Initramfs => "initramfs",
        }
    }

    fn cache_limit(self) -> u64 {
        match self {
            ImageBlobKind::BaseImage => MAX_BASE_IMAGE_BYTES,
            ImageBlobKind::Kernel => MAX_KERNEL_BYTES,
            ImageBlobKind::Initramfs => MAX_INITRAMFS_BYTES,
        }
    }
}

#[derive(Debug)]
pub enum ImageResolverError {
    InvalidConfig(ConfigError),
    NotFound {
        kind: ImageBlobKind,
        hash: [u8; 32],
        path: PathBuf,
    },
    NotFile {
        kind: ImageBlobKind,
        path: PathBuf,
    },
    HashMismatch {
        kind: ImageBlobKind,
        path: PathBuf,
        expected: [u8; 32],
        actual: [u8; 32],
    },
    TooLarge {
        kind: ImageBlobKind,
        path: PathBuf,
        len: u64,
        max: u64,
    },
    AllocationFailed {
        kind: ImageBlobKind,
        path: PathBuf,
        requested: u64,
    },
    Io {
        kind: ImageBlobKind,
        path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for ImageResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageResolverError::InvalidConfig(e) => write!(f, "invalid MachineConfig: {e:?}"),
            ImageResolverError::NotFound { kind, hash, path } => write!(
                f,
                "{} {} not found at {}",
                kind.label(),
                cache_key(hash),
                path.display()
            ),
            ImageResolverError::NotFile { kind, path } => {
                write!(
                    f,
                    "{} path is not a regular file: {}",
                    kind.label(),
                    path.display()
                )
            }
            ImageResolverError::HashMismatch {
                kind,
                path,
                expected,
                actual,
            } => write!(
                f,
                "{} hash mismatch at {}: expected {}, got {}",
                kind.label(),
                path.display(),
                cache_key(expected),
                cache_key(actual)
            ),
            ImageResolverError::TooLarge {
                kind,
                path,
                len,
                max,
            } => write!(
                f,
                "{} at {} is {len} bytes, exceeds {max} byte cap",
                kind.label(),
                path.display()
            ),
            ImageResolverError::AllocationFailed {
                kind,
                path,
                requested,
            } => write!(
                f,
                "failed to allocate {requested} bytes for {} at {}",
                kind.label(),
                path.display()
            ),
            ImageResolverError::Io { kind, path, source } => {
                write!(f, "{} I/O at {}: {source}", kind.label(), path.display())
            }
        }
    }
}

impl std::error::Error for ImageResolverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ImageResolverError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ImageResolver {
    root: PathBuf,
}

impl ImageResolver {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn from_default_cache() -> Self {
        Self::new(DEFAULT_IMAGE_CACHE_DIR)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn blob_path(&self, hash: &[u8; 32]) -> PathBuf {
        self.root.join(cache_key(hash))
    }

    pub fn resolve_create_vm(
        &self,
        config: &MachineConfig,
    ) -> Result<CreateVmAssets, ImageResolverError> {
        config
            .validate()
            .map_err(ImageResolverError::InvalidConfig)?;
        let (base_image_path, base_image) = self.open_base_image(&config.base_image_hash)?;
        let boot = match &config.boot {
            BootSpec::Elf {
                kernel_hash,
                cmdline,
            } => ResolvedBoot::Elf {
                kernel: self.read_blob(ImageBlobKind::Kernel, kernel_hash)?,
                cmdline: cmdline.clone(),
            },
            BootSpec::BzImage {
                kernel_hash,
                initramfs_hash,
                cmdline,
            } => ResolvedBoot::BzImage {
                kernel: self.read_blob(ImageBlobKind::Kernel, kernel_hash)?,
                initramfs: self.read_blob(ImageBlobKind::Initramfs, initramfs_hash)?,
                cmdline: cmdline.clone(),
            },
        };
        Ok(CreateVmAssets {
            base_image_path,
            base_image,
            boot,
        })
    }

    pub fn open_base_image(
        &self,
        hash: &[u8; 32],
    ) -> Result<(PathBuf, FileBase), ImageResolverError> {
        let (path, bytes) =
            self.read_verified_blob_limited(ImageBlobKind::BaseImage, hash, MAX_BASE_IMAGE_BYTES)?;
        Ok((path, FileBase::from_owned_bytes(bytes)))
    }

    fn read_blob(
        &self,
        kind: ImageBlobKind,
        hash: &[u8; 32],
    ) -> Result<Vec<u8>, ImageResolverError> {
        match kind {
            ImageBlobKind::Kernel | ImageBlobKind::Initramfs => {
                self.read_blob_limited(kind, hash, kind.cache_limit())
            }
            ImageBlobKind::BaseImage => {
                panic!("read_blob is only for boot blobs; use open_base_image")
            }
        }
    }

    fn read_blob_limited(
        &self,
        kind: ImageBlobKind,
        expected: &[u8; 32],
        max_bytes: u64,
    ) -> Result<Vec<u8>, ImageResolverError> {
        self.read_verified_blob_limited(kind, expected, max_bytes)
            .map(|(_path, bytes)| bytes)
    }

    fn read_verified_blob_limited(
        &self,
        kind: ImageBlobKind,
        expected: &[u8; 32],
        max_bytes: u64,
    ) -> Result<(PathBuf, Vec<u8>), ImageResolverError> {
        let (path, mut file, len) = self.open_cache_file(kind, expected)?;
        if len > max_bytes {
            return Err(ImageResolverError::TooLarge {
                kind,
                path: path.clone(),
                len,
                max: max_bytes,
            });
        }
        let initial_capacity =
            usize::try_from(len).map_err(|_| ImageResolverError::AllocationFailed {
                kind,
                path: path.clone(),
                requested: len,
            })?;
        let mut out = Vec::new();
        out.try_reserve_exact(initial_capacity).map_err(|_| {
            ImageResolverError::AllocationFailed {
                kind,
                path: path.clone(),
                requested: len,
            }
        })?;
        let mut hasher = blake3::Hasher::new();
        let mut total = 0u64;
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = file
                .read(&mut buf)
                .map_err(|source| ImageResolverError::Io {
                    kind,
                    path: path.clone(),
                    source,
                })?;
            if n == 0 {
                break;
            }
            total = total
                .checked_add(n as u64)
                .ok_or_else(|| ImageResolverError::TooLarge {
                    kind,
                    path: path.clone(),
                    len: u64::MAX,
                    max: max_bytes,
                })?;
            if total > max_bytes {
                return Err(ImageResolverError::TooLarge {
                    kind,
                    path: path.clone(),
                    len: total,
                    max: max_bytes,
                });
            }
            let required =
                out.len()
                    .checked_add(n)
                    .ok_or_else(|| ImageResolverError::AllocationFailed {
                        kind,
                        path: path.clone(),
                        requested: u64::MAX,
                    })?;
            if required > out.capacity() {
                let extra = required - out.capacity();
                out.try_reserve_exact(extra)
                    .map_err(|_| ImageResolverError::AllocationFailed {
                        kind,
                        path: path.clone(),
                        requested: total,
                    })?;
            }
            hasher.update(&buf[..n]);
            out.extend_from_slice(&buf[..n]);
        }
        let actual = *hasher.finalize().as_bytes();
        if actual != *expected {
            return Err(ImageResolverError::HashMismatch {
                kind,
                path,
                expected: *expected,
                actual,
            });
        }
        Ok((path, out))
    }

    fn open_cache_file(
        &self,
        kind: ImageBlobKind,
        expected: &[u8; 32],
    ) -> Result<(PathBuf, File, u64), ImageResolverError> {
        let path = self.blob_path(expected);
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
        {
            Ok(file) => file,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Err(ImageResolverError::NotFound {
                    kind,
                    hash: *expected,
                    path,
                });
            }
            Err(source) if source.raw_os_error() == Some(libc::ELOOP) => {
                return Err(ImageResolverError::NotFile { kind, path });
            }
            Err(source) => return Err(ImageResolverError::Io { kind, path, source }),
        };
        let meta = file.metadata().map_err(|source| ImageResolverError::Io {
            kind,
            path: path.clone(),
            source,
        })?;
        if !meta.file_type().is_file() {
            return Err(ImageResolverError::NotFile { kind, path });
        }
        Ok((path, file, meta.len()))
    }
}

pub struct CreateVmAssets {
    pub base_image_path: PathBuf,
    pub base_image: FileBase,
    pub boot: ResolvedBoot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedBoot {
    Elf {
        kernel: Vec<u8>,
        cmdline: Vec<u8>,
    },
    BzImage {
        kernel: Vec<u8>,
        initramfs: Vec<u8>,
        cmdline: Vec<u8>,
    },
}

pub fn cache_key(hash: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for b in hash {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dh_devices::blk::{
        BlockBase, PvBlk, CMD_READ, CMD_WRITE, REG_BUF_GPA, REG_CMD, REG_COUNT, REG_SECTOR,
        REG_STATUS, SECTOR_SIZE, STATUS_OK,
    };
    use dh_devices::ctx::VecGuestMem;
    use dh_devices::{DetDevice, EntropySource};
    use dh_inputlog::dhilog::{LogWriter, SegmentHeader};
    use std::io::Write;

    struct NoEntropy;
    impl EntropySource for NoEntropy {
        fn fill(&mut self, _buf: &mut [u8]) {}
    }

    struct CacheDir {
        path: PathBuf,
    }

    impl CacheDir {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "dh-worker-image-cache-{}-{tag}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn write_blob(&self, bytes: &[u8]) -> [u8; 32] {
            let hash = *blake3::hash(bytes).as_bytes();
            let mut file = File::create(self.path.join(cache_key(&hash))).unwrap();
            file.write_all(bytes).unwrap();
            file.sync_all().unwrap();
            hash
        }

        fn write_at_hash(&self, hash: &[u8; 32], bytes: &[u8]) {
            let mut file = File::create(self.path.join(cache_key(hash))).unwrap();
            file.write_all(bytes).unwrap();
            file.sync_all().unwrap();
        }

        fn set_len_at_hash(&self, hash: &[u8; 32], len: u64) {
            let file = File::options()
                .write(true)
                .open(self.path.join(cache_key(hash)))
                .unwrap();
            file.set_len(len).unwrap();
            file.sync_all().unwrap();
        }

        fn create_sparse_at_hash(&self, hash: &[u8; 32], len: u64) {
            let file = File::create(self.path.join(cache_key(hash))).unwrap();
            file.set_len(len).unwrap();
            file.sync_all().unwrap();
        }
    }

    impl Drop for CacheDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn request(
        dev: &mut PvBlk,
        mem: &mut VecGuestMem,
        cmd: u32,
        sector: u64,
        gpa: u64,
        n: u32,
    ) -> u32 {
        let mut log = LogWriter::new(SegmentHeader {
            base_snapshot_id: [0; 32],
            entropy_seed: [0; 32],
            machine_config_hash: [0; 32],
            clock_num: 1,
            clock_den: 1,
            encoder_fingerprint: 0,
        });
        let mut ent = NoEntropy;
        let mut q = Vec::new();
        let mut ctx = dh_devices::DevCtx::new(1, 0, &mut log, mem, &mut ent, &mut q);
        dev.mmio_write(REG_SECTOR, &sector.to_le_bytes(), &mut ctx);
        dev.mmio_write(REG_BUF_GPA, &gpa.to_le_bytes(), &mut ctx);
        dev.mmio_write(REG_COUNT, &n.to_le_bytes(), &mut ctx);
        dev.mmio_write(REG_CMD, &cmd.to_le_bytes(), &mut ctx);
        let mut s = [0u8; 4];
        dev.mmio_read(REG_STATUS, &mut s, &mut ctx);
        u32::from_le_bytes(s)
    }

    #[test]
    fn resolves_base_image_and_elf_boot_bytes_by_verified_hash() {
        let cache = CacheDir::new("elf");
        let base = b"base image bytes";
        let kernel = b"kernel bytes";
        let base_hash = cache.write_blob(base);
        let kernel_hash = cache.write_blob(kernel);
        let config = MachineConfig::new(
            64 * 1024 * 1024,
            base_hash,
            BootSpec::Elf {
                kernel_hash,
                cmdline: b"console=none".to_vec(),
            },
        );

        let assets = ImageResolver::new(&cache.path)
            .resolve_create_vm(&config)
            .unwrap();
        assert_eq!(
            assets.base_image_path,
            cache.path.join(cache_key(&base_hash))
        );
        assert_eq!(assets.base_image.len(), base.len() as u64);
        let mut got_base = vec![0; base.len()];
        assets.base_image.read_at(0, &mut got_base).unwrap();
        assert_eq!(got_base, base);
        assert_eq!(
            assets.boot,
            ResolvedBoot::Elf {
                kernel: kernel.to_vec(),
                cmdline: b"console=none".to_vec(),
            }
        );
    }

    #[test]
    fn base_image_is_owned_after_verification() {
        let cache = CacheDir::new("base-owned-after-verify");
        let original = b"base image original bytes for owned backing";
        let hash = cache.write_blob(original);

        let (_path, base) = ImageResolver::new(&cache.path)
            .open_base_image(&hash)
            .unwrap();

        cache.write_at_hash(&hash, b"mutated cache bytes");
        let mut got = vec![0; original.len()];
        base.read_at(0, &mut got).unwrap();
        assert_eq!(got, original);

        cache.set_len_at_hash(&hash, 0);
        let mut got_again = vec![0; original.len()];
        base.read_at(0, &mut got_again).unwrap();
        assert_eq!(got_again, original);
    }

    #[test]
    fn pvblk_reads_owned_base_after_cache_overwrite_and_truncate() {
        let cache = CacheDir::new("base-owned-pvblk");
        let original: Vec<u8> = (0..2 * SECTOR_SIZE).map(|i| (i % 251) as u8).collect();
        let hash = cache.write_blob(&original);

        let (_path, base) = ImageResolver::new(&cache.path)
            .open_base_image(&hash)
            .unwrap();
        let mut dev = PvBlk::new(Box::new(base));
        let mut mem = VecGuestMem(vec![0u8; 0x5000]);

        cache.write_at_hash(&hash, &vec![0xA5; original.len()]);
        assert_eq!(
            request(&mut dev, &mut mem, CMD_READ, 0, 0x1000, 1),
            STATUS_OK
        );
        assert_eq!(
            &mem.0[0x1000..0x1000 + SECTOR_SIZE],
            &original[..SECTOR_SIZE]
        );

        cache.set_len_at_hash(&hash, 0);
        mem.0[0x2000..0x2000 + SECTOR_SIZE].fill(0xDD);
        assert_eq!(
            request(&mut dev, &mut mem, CMD_WRITE, 0, 0x2000, 1),
            STATUS_OK
        );
        assert_eq!(
            request(&mut dev, &mut mem, CMD_READ, 1, 0x3000, 1),
            STATUS_OK
        );
        assert_eq!(
            &mem.0[0x3000..0x3000 + SECTOR_SIZE],
            &original[SECTOR_SIZE..2 * SECTOR_SIZE],
            "RMW must use the verified owned base bytes, not the mutated cache inode"
        );
    }

    #[test]
    fn resolves_bzimage_kernel_and_initramfs_bytes() {
        let cache = CacheDir::new("bzimage");
        let base_hash = cache.write_blob(b"base");
        let kernel_hash = cache.write_blob(b"bzimage");
        let initramfs_hash = cache.write_blob(b"initramfs");
        let config = MachineConfig::new(
            64 * 1024 * 1024,
            base_hash,
            BootSpec::BzImage {
                kernel_hash,
                initramfs_hash,
                cmdline: dh_vmm::config::canonicalize_bzimage_cmdline_extras(b"quiet").unwrap(),
            },
        );

        let assets = ImageResolver::new(&cache.path)
            .resolve_create_vm(&config)
            .unwrap();
        assert_eq!(
            assets.boot,
            ResolvedBoot::BzImage {
                kernel: b"bzimage".to_vec(),
                initramfs: b"initramfs".to_vec(),
                cmdline: dh_vmm::config::canonicalize_bzimage_cmdline_extras(b"quiet").unwrap(),
            }
        );
    }

    #[test]
    fn missing_bzimage_initramfs_is_not_found_before_boot_escapes() {
        let cache = CacheDir::new("missing-initramfs");
        let base_hash = cache.write_blob(b"base");
        let kernel_hash = cache.write_blob(b"bzimage");
        let missing_initramfs = [0xB7; 32];
        let config = MachineConfig::new(
            64 * 1024 * 1024,
            base_hash,
            BootSpec::BzImage {
                kernel_hash,
                initramfs_hash: missing_initramfs,
                cmdline: dh_vmm::config::canonicalize_bzimage_cmdline_extras(b"quiet").unwrap(),
            },
        );

        match ImageResolver::new(&cache.path).resolve_create_vm(&config) {
            Err(ImageResolverError::NotFound {
                kind: ImageBlobKind::Initramfs,
                hash,
                ..
            }) => assert_eq!(hash, missing_initramfs),
            Err(e) => panic!("wrong error: {e}"),
            Ok(_) => panic!("unexpected resolver success"),
        }
    }

    #[test]
    fn unknown_hash_is_not_found() {
        let cache = CacheDir::new("missing");
        let base_hash = cache.write_blob(b"base");
        let missing_kernel = [0xA5; 32];
        let config = MachineConfig::new(
            64 * 1024 * 1024,
            base_hash,
            BootSpec::Elf {
                kernel_hash: missing_kernel,
                cmdline: Vec::new(),
            },
        );

        let result = ImageResolver::new(&cache.path).resolve_create_vm(&config);
        match result {
            Err(ImageResolverError::NotFound {
                kind: ImageBlobKind::Kernel,
                hash,
                ..
            }) => assert_eq!(hash, missing_kernel),
            Err(e) => panic!("wrong error: {e}"),
            Ok(_) => panic!("unexpected resolver success"),
        }
    }

    #[test]
    fn hash_mismatch_is_loud_before_bytes_escape() {
        let cache = CacheDir::new("mismatch");
        let expected = [0x5A; 32];
        cache.write_at_hash(&expected, b"tampered");
        let actual = *blake3::hash(b"tampered").as_bytes();

        match ImageResolver::new(&cache.path).read_blob(ImageBlobKind::Kernel, &expected) {
            Err(ImageResolverError::HashMismatch {
                kind: ImageBlobKind::Kernel,
                expected: got_expected,
                actual: got_actual,
                ..
            }) => {
                assert_eq!(got_expected, expected);
                assert_eq!(got_actual, actual);
            }
            Ok(_) => panic!("unexpected resolver success"),
            Err(e) => panic!("wrong error: {e}"),
        }
    }

    #[test]
    fn base_image_hash_mismatch_is_loud_before_bytes_escape() {
        let cache = CacheDir::new("base-mismatch");
        let expected = [0x5C; 32];
        cache.write_at_hash(&expected, b"tampered-base");
        let actual = *blake3::hash(b"tampered-base").as_bytes();

        match ImageResolver::new(&cache.path).open_base_image(&expected) {
            Err(ImageResolverError::HashMismatch {
                kind: ImageBlobKind::BaseImage,
                expected: got_expected,
                actual: got_actual,
                ..
            }) => {
                assert_eq!(got_expected, expected);
                assert_eq!(got_actual, actual);
            }
            Ok(_) => panic!("unexpected resolver success"),
            Err(e) => panic!("wrong error: {e}"),
        }
    }

    #[test]
    fn bzimage_initramfs_hash_mismatch_is_loud_before_boot_escapes() {
        let cache = CacheDir::new("initramfs-mismatch");
        let base_hash = cache.write_blob(b"base");
        let kernel_hash = cache.write_blob(b"bzimage");
        let expected_initramfs = [0x5B; 32];
        cache.write_at_hash(&expected_initramfs, b"tampered-initramfs");
        let actual_initramfs = *blake3::hash(b"tampered-initramfs").as_bytes();
        let config = MachineConfig::new(
            64 * 1024 * 1024,
            base_hash,
            BootSpec::BzImage {
                kernel_hash,
                initramfs_hash: expected_initramfs,
                cmdline: dh_vmm::config::canonicalize_bzimage_cmdline_extras(b"quiet").unwrap(),
            },
        );

        match ImageResolver::new(&cache.path).resolve_create_vm(&config) {
            Err(ImageResolverError::HashMismatch {
                kind: ImageBlobKind::Initramfs,
                expected,
                actual,
                ..
            }) => {
                assert_eq!(expected, expected_initramfs);
                assert_eq!(actual, actual_initramfs);
            }
            Err(e) => panic!("wrong error: {e}"),
            Ok(_) => panic!("unexpected resolver success"),
        }
    }

    #[test]
    fn oversized_boot_blobs_are_rejected_before_hashing() {
        let cache = CacheDir::new("too-large");
        let expected_kernel = [0x61; 32];
        cache.write_at_hash(&expected_kernel, b"12345");
        match ImageResolver::new(&cache.path).read_blob_limited(
            ImageBlobKind::Kernel,
            &expected_kernel,
            4,
        ) {
            Err(ImageResolverError::TooLarge {
                kind: ImageBlobKind::Kernel,
                len: 5,
                max: 4,
                ..
            }) => {}
            other => panic!("wrong result: {other:?}"),
        }

        let expected_initramfs = [0x62; 32];
        cache.write_at_hash(&expected_initramfs, b"abcdef");
        match ImageResolver::new(&cache.path).read_blob_limited(
            ImageBlobKind::Initramfs,
            &expected_initramfs,
            5,
        ) {
            Err(ImageResolverError::TooLarge {
                kind: ImageBlobKind::Initramfs,
                len: 6,
                max: 5,
                ..
            }) => {}
            other => panic!("wrong result: {other:?}"),
        }
    }

    #[test]
    fn boot_blob_caps_use_contract_constants() {
        let cache = CacheDir::new("contract-caps");
        let expected_kernel = [0x63; 32];
        cache.create_sparse_at_hash(&expected_kernel, MAX_KERNEL_BYTES + 1);
        match ImageResolver::new(&cache.path).read_blob(ImageBlobKind::Kernel, &expected_kernel) {
            Err(ImageResolverError::TooLarge {
                kind: ImageBlobKind::Kernel,
                len,
                max,
                ..
            }) => {
                assert_eq!(len, MAX_KERNEL_BYTES + 1);
                assert_eq!(max, MAX_KERNEL_BYTES);
            }
            other => panic!("wrong result: {other:?}"),
        }

        let expected_initramfs = [0x64; 32];
        cache.create_sparse_at_hash(&expected_initramfs, MAX_INITRAMFS_BYTES + 1);
        match ImageResolver::new(&cache.path)
            .read_blob(ImageBlobKind::Initramfs, &expected_initramfs)
        {
            Err(ImageResolverError::TooLarge {
                kind: ImageBlobKind::Initramfs,
                len,
                max,
                ..
            }) => {
                assert_eq!(len, MAX_INITRAMFS_BYTES + 1);
                assert_eq!(max, MAX_INITRAMFS_BYTES);
            }
            other => panic!("wrong result: {other:?}"),
        }
    }

    #[test]
    fn base_image_cap_rejects_sparse_entry_before_hashing() {
        let cache = CacheDir::new("base-too-large");
        let expected = [0x65; 32];
        cache.create_sparse_at_hash(&expected, MAX_BASE_IMAGE_BYTES + 1);

        match ImageResolver::new(&cache.path).open_base_image(&expected) {
            Err(ImageResolverError::TooLarge {
                kind: ImageBlobKind::BaseImage,
                len,
                max,
                ..
            }) => {
                assert_eq!(len, MAX_BASE_IMAGE_BYTES + 1);
                assert_eq!(max, MAX_BASE_IMAGE_BYTES);
            }
            Ok(_) => panic!("unexpected resolver success"),
            Err(e) => panic!("wrong error: {e}"),
        }
    }

    #[test]
    fn verified_read_rejects_sparse_metadata_before_hashing() {
        let cache = CacheDir::new("base-small-limit-sparse");
        let expected = [0x66; 32];
        cache.create_sparse_at_hash(&expected, 1024 * 1024);

        match ImageResolver::new(&cache.path).read_verified_blob_limited(
            ImageBlobKind::BaseImage,
            &expected,
            4,
        ) {
            Err(ImageResolverError::TooLarge {
                kind: ImageBlobKind::BaseImage,
                len,
                max,
                ..
            }) => {
                assert_eq!(len, 1024 * 1024);
                assert_eq!(max, 4);
            }
            other => panic!("wrong result: {other:?}"),
        }
    }

    #[test]
    fn cache_entries_must_be_regular_non_symlink_files() {
        let cache = CacheDir::new("not-file");
        let dir_hash = [0x71; 32];
        std::fs::create_dir(cache.path.join(cache_key(&dir_hash))).unwrap();
        match ImageResolver::new(&cache.path).read_blob_limited(
            ImageBlobKind::Kernel,
            &dir_hash,
            1024,
        ) {
            Err(ImageResolverError::NotFile {
                kind: ImageBlobKind::Kernel,
                ..
            }) => {}
            other => panic!("wrong result for directory entry: {other:?}"),
        }

        let link_hash = [0x72; 32];
        let target = cache.path.join("outside-target");
        File::create(&target).unwrap().write_all(b"target").unwrap();
        std::os::unix::fs::symlink(&target, cache.path.join(cache_key(&link_hash))).unwrap();
        match ImageResolver::new(&cache.path).read_blob_limited(
            ImageBlobKind::Kernel,
            &link_hash,
            1024,
        ) {
            Err(ImageResolverError::NotFile {
                kind: ImageBlobKind::Kernel,
                ..
            }) => {}
            other => panic!("wrong result for symlink entry: {other:?}"),
        }

        let initramfs_dir_hash = [0x73; 32];
        std::fs::create_dir(cache.path.join(cache_key(&initramfs_dir_hash))).unwrap();
        match ImageResolver::new(&cache.path).read_blob_limited(
            ImageBlobKind::Initramfs,
            &initramfs_dir_hash,
            1024,
        ) {
            Err(ImageResolverError::NotFile {
                kind: ImageBlobKind::Initramfs,
                ..
            }) => {}
            other => panic!("wrong result for initramfs directory entry: {other:?}"),
        }

        let initramfs_link_hash = [0x74; 32];
        std::os::unix::fs::symlink(&target, cache.path.join(cache_key(&initramfs_link_hash)))
            .unwrap();
        match ImageResolver::new(&cache.path).read_blob_limited(
            ImageBlobKind::Initramfs,
            &initramfs_link_hash,
            1024,
        ) {
            Err(ImageResolverError::NotFile {
                kind: ImageBlobKind::Initramfs,
                ..
            }) => {}
            other => panic!("wrong result for initramfs symlink entry: {other:?}"),
        }

        let base_dir_hash = [0x75; 32];
        std::fs::create_dir(cache.path.join(cache_key(&base_dir_hash))).unwrap();
        match ImageResolver::new(&cache.path).open_base_image(&base_dir_hash) {
            Err(ImageResolverError::NotFile {
                kind: ImageBlobKind::BaseImage,
                ..
            }) => {}
            Ok(_) => panic!("unexpected resolver success for base directory entry"),
            Err(e) => panic!("wrong error for base directory entry: {e}"),
        }

        let base_link_hash = [0x76; 32];
        std::os::unix::fs::symlink(&target, cache.path.join(cache_key(&base_link_hash))).unwrap();
        match ImageResolver::new(&cache.path).open_base_image(&base_link_hash) {
            Err(ImageResolverError::NotFile {
                kind: ImageBlobKind::BaseImage,
                ..
            }) => {}
            Ok(_) => panic!("unexpected resolver success for base symlink entry"),
            Err(e) => panic!("wrong error for base symlink entry: {e}"),
        }
    }
}
