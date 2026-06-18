//! CreateVm image-cache resolver (bead p8g).
//!
//! Public CreateVm carries content hashes, while the boot and pv-blk paths
//! consume deterministic bytes/files. This module is the daemon-owned seam
//! between those contracts: a flat local cache keyed by lowercase BLAKE3 hex,
//! with every opened blob verified before it reaches KVM/device setup.

use dh_vmm::blkfile::FileBase;
use dh_vmm::config::{BootSpec, ConfigError, MachineConfig};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub const DEFAULT_IMAGE_CACHE_DIR: &str = "/var/lib/dh/images";
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

    fn boot_blob_limit(self) -> Option<u64> {
        match self {
            ImageBlobKind::BaseImage => None,
            ImageBlobKind::Kernel => Some(MAX_KERNEL_BYTES),
            ImageBlobKind::Initramfs => Some(MAX_INITRAMFS_BYTES),
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
        let kind = ImageBlobKind::BaseImage;
        let (path, file) = self.open_verified_file(kind, hash)?;
        let base = FileBase::from_file(file).map_err(|source| ImageResolverError::Io {
            kind,
            path: path.clone(),
            source,
        })?;
        Ok((path, base))
    }

    fn read_blob(
        &self,
        kind: ImageBlobKind,
        hash: &[u8; 32],
    ) -> Result<Vec<u8>, ImageResolverError> {
        let limit = kind
            .boot_blob_limit()
            .expect("read_blob is only for boot blobs");
        self.read_blob_limited(kind, hash, limit)
    }

    fn read_blob_limited(
        &self,
        kind: ImageBlobKind,
        expected: &[u8; 32],
        max_bytes: u64,
    ) -> Result<Vec<u8>, ImageResolverError> {
        let (path, mut file, len) = self.open_cache_file(kind, expected)?;
        if len > max_bytes {
            return Err(ImageResolverError::TooLarge {
                kind,
                path,
                len,
                max: max_bytes,
            });
        }
        let mut out = Vec::with_capacity(len as usize);
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
                    path,
                    len: total,
                    max: max_bytes,
                });
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
        Ok(out)
    }

    fn open_verified_file(
        &self,
        kind: ImageBlobKind,
        expected: &[u8; 32],
    ) -> Result<(PathBuf, File), ImageResolverError> {
        let (path, mut file, _) = self.open_cache_file(kind, expected)?;
        let actual = hash_file(&mut file).map_err(|source| ImageResolverError::Io {
            kind,
            path: path.clone(),
            source,
        })?;
        if actual != *expected {
            return Err(ImageResolverError::HashMismatch {
                kind,
                path,
                expected: *expected,
                actual,
            });
        }
        Ok((path, file))
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

fn hash_file(file: &mut File) -> std::io::Result<[u8; 32]> {
    file.seek(SeekFrom::Start(0))?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dh_devices::blk::BlockBase;
    use std::io::Write;

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
            other => panic!("wrong result: {other:?}"),
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
    }
}
