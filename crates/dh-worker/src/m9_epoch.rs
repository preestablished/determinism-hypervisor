//! Fail-closed image/epoch identity guard (plan epoch-023, contract S2).
//!
//! The M9 corpus manifest (`expected.txt`) pins the reference-workload bundle
//! only by artifact content hashes; nothing else in the worker names the
//! emulator epoch. This module is the one place that compares what is staged
//! on the host (`DH_M9_*` files plus the operator-written `staging-stamp.txt`
//! beside `bzImage`) against those pins, and refuses on any difference.
//!
//! Everything here is host-side parsing over `key=value` text so it is unit
//! tested on every arch. Mismatch reports carry KEY NAMES ONLY (never hash
//! values) so they can appear in public summaries.

use std::fmt;
use std::fs;
use std::path::Path;

/// Keys the corpus manifest must carry for the guard.
pub const CORPUS_HASH_KEYS: [&str; 4] = [
    "bzimage_blake3",
    "initramfs_blake3",
    "base_image_blake3",
    "game_image_blake3",
];
/// Emulator epoch key written into the corpus manifest by the regen at
/// bundle 0.2.x and later (absent in pre-epoch manifests).
pub const CORPUS_EMU_VERSION_KEY: &str = "emu_version";

/// The four staged artifact hashes as the worker computed them while
/// populating the image cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StagedArtifactHashes {
    pub bzimage: [u8; 32],
    pub initramfs: [u8; 32],
    pub base_image: [u8; 32],
    pub game_image: [u8; 32],
}

/// Pins read from the corpus `expected.txt`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorpusPins {
    pub bzimage: [u8; 32],
    pub initramfs: [u8; 32],
    pub base_image: [u8; 32],
    pub game_image: [u8; 32],
    /// `emu_version=` value; `None` for pre-epoch manifests.
    pub emu_version: Option<String>,
}

/// `staging-stamp.txt` written by the operator staging step (plan B WP1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagingStamp {
    pub bundle_version: String,
    pub manifest_git_rev: String,
    pub manifest_emu_version: String,
    pub manifest_kernel_blake3: [u8; 32],
    pub manifest_initramfs_zst_blake3: Option<[u8; 32]>,
    pub staged_bzimage_blake3: Option<[u8; 32]>,
    pub staged_initramfs_cpio_blake3: Option<[u8; 32]>,
    pub staged_initramfs_zst_blake3: Option<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageIdentityVerdict {
    Match,
    /// Names of the keys that disagree, in a fixed order; never values.
    Mismatch(Vec<&'static str>),
}

impl ImageIdentityVerdict {
    pub fn is_match(&self) -> bool {
        matches!(self, Self::Match)
    }

    pub fn mismatch_keys(&self) -> &[&'static str] {
        match self {
            Self::Match => &[],
            Self::Mismatch(keys) => keys,
        }
    }
}

impl fmt::Display for ImageIdentityVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Match => write!(f, "match"),
            Self::Mismatch(keys) => write!(f, "mismatch({})", keys.join(",")),
        }
    }
}

/// Minimal `key=value` reader: blank lines and `#` comments skipped, first
/// `=` splits, later duplicates rejected. Values are trimmed.
pub fn parse_kv(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("line {}: missing '='", idx + 1));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("line {}: empty key", idx + 1));
        }
        if out.iter().any(|(k, _)| k == key) {
            return Err(format!("line {}: duplicate key {key}", idx + 1));
        }
        out.push((key.to_owned(), value.trim().to_owned()));
    }
    Ok(out)
}

fn lookup<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// Lowercase 64-hex BLAKE3 → bytes; error names the key.
pub fn parse_hex32(key: &str, value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{key}: expected 64 hex characters"));
    }
    let mut out = [0u8; 32];
    for (i, chunk) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let s = std::str::from_utf8(chunk).map_err(|_| format!("{key}: invalid hex"))?;
        out[i] = u8::from_str_radix(s, 16).map_err(|_| format!("{key}: invalid hex"))?;
    }
    Ok(out)
}

fn required_hex32(pairs: &[(String, String)], key: &str) -> Result<[u8; 32], String> {
    let value = lookup(pairs, key).ok_or_else(|| format!("{key}: missing"))?;
    parse_hex32(key, value)
}

fn optional_hex32(pairs: &[(String, String)], key: &str) -> Result<Option<[u8; 32]>, String> {
    match lookup(pairs, key) {
        None | Some("") => Ok(None),
        Some(value) => parse_hex32(key, value).map(Some),
    }
}

fn required_string(pairs: &[(String, String)], key: &str) -> Result<String, String> {
    match lookup(pairs, key) {
        Some(value) if !value.is_empty() => Ok(value.to_owned()),
        Some(_) => Err(format!("{key}: empty")),
        None => Err(format!("{key}: missing")),
    }
}

/// Parse the corpus manifest. Unknown keys are ignored (the manifest carries
/// many replay pins this guard does not own).
pub fn parse_corpus_pins(text: &str) -> Result<CorpusPins, String> {
    let pairs = parse_kv(text)?;
    Ok(CorpusPins {
        bzimage: required_hex32(&pairs, CORPUS_HASH_KEYS[0])?,
        initramfs: required_hex32(&pairs, CORPUS_HASH_KEYS[1])?,
        base_image: required_hex32(&pairs, CORPUS_HASH_KEYS[2])?,
        game_image: required_hex32(&pairs, CORPUS_HASH_KEYS[3])?,
        emu_version: match lookup(&pairs, CORPUS_EMU_VERSION_KEY) {
            Some(value) if !value.is_empty() => Some(value.to_owned()),
            Some(_) => return Err(format!("{CORPUS_EMU_VERSION_KEY}: empty")),
            None => None,
        },
    })
}

/// Parse `staging-stamp.txt`. The manifest identity keys are required; the
/// staged-file hashes are optional (older stamps) but checked when present.
pub fn parse_staging_stamp(text: &str) -> Result<StagingStamp, String> {
    let pairs = parse_kv(text)?;
    Ok(StagingStamp {
        bundle_version: required_string(&pairs, "bundle_version")?,
        manifest_git_rev: required_string(&pairs, "manifest_git_rev")?,
        manifest_emu_version: required_string(&pairs, "manifest_emu_version")?,
        manifest_kernel_blake3: required_hex32(&pairs, "manifest_kernel_blake3")?,
        manifest_initramfs_zst_blake3: optional_hex32(&pairs, "manifest_initramfs_zst_blake3")?,
        staged_bzimage_blake3: optional_hex32(&pairs, "staged_bzimage_blake3")?,
        staged_initramfs_cpio_blake3: optional_hex32(&pairs, "staged_initramfs_cpio_blake3")?,
        staged_initramfs_zst_blake3: optional_hex32(&pairs, "staged_initramfs_zst_blake3")?,
    })
}

/// Compare, in order: the four staged hashes against the corpus pins; then,
/// when a stamp is given, the manifest's kernel hash against the staged
/// `bzImage`, the manifest's `.zst` hash against the staged `.zst` (when both
/// are recorded), and the manifest's emulator version against the corpus
/// `emu_version` (a pre-epoch manifest without the key is a mismatch — that is
/// exactly the sanctioned `--allow-image-change` re-baseline run).
pub fn check_image_identity(
    pins: &CorpusPins,
    staged: &StagedArtifactHashes,
    stamp: Option<&StagingStamp>,
) -> ImageIdentityVerdict {
    let mut keys = Vec::new();
    if pins.bzimage != staged.bzimage {
        keys.push(CORPUS_HASH_KEYS[0]);
    }
    if pins.initramfs != staged.initramfs {
        keys.push(CORPUS_HASH_KEYS[1]);
    }
    if pins.base_image != staged.base_image {
        keys.push(CORPUS_HASH_KEYS[2]);
    }
    if pins.game_image != staged.game_image {
        keys.push(CORPUS_HASH_KEYS[3]);
    }
    if let Some(stamp) = stamp {
        if stamp.manifest_kernel_blake3 != staged.bzimage {
            keys.push("manifest_kernel_blake3");
        }
        if let (Some(manifest), Some(staged_zst)) = (
            stamp.manifest_initramfs_zst_blake3,
            stamp.staged_initramfs_zst_blake3,
        ) {
            if manifest != staged_zst {
                keys.push("manifest_initramfs_zst_blake3");
            }
        }
        if pins.emu_version.as_deref() != Some(stamp.manifest_emu_version.as_str()) {
            keys.push(CORPUS_EMU_VERSION_KEY);
        }
    }
    if keys.is_empty() {
        ImageIdentityVerdict::Match
    } else {
        ImageIdentityVerdict::Mismatch(keys)
    }
}

/// `<bundle version>@<blake3 of the stamp file bytes>` — the value
/// `GetWorkerInfoResponse.image_identity` serves so consumers can assert the
/// epoch at connect time.
pub fn image_identity_string(stamp: &StagingStamp, stamp_bytes: &[u8]) -> String {
    format!(
        "{}@{}",
        stamp.bundle_version,
        blake3::hash(stamp_bytes).to_hex()
    )
}

/// The stamp read from disk together with its raw bytes and location.
#[derive(Clone, Debug)]
pub struct LoadedStamp {
    pub stamp: StagingStamp,
    pub bytes: Vec<u8>,
    pub dir: std::path::PathBuf,
}

impl LoadedStamp {
    pub fn identity(&self) -> String {
        image_identity_string(&self.stamp, &self.bytes)
    }
}

pub fn load_staging_stamp(path: &Path) -> Result<LoadedStamp, String> {
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let text = std::str::from_utf8(&bytes).map_err(|_| format!("{}: not utf-8", path.display()))?;
    let stamp = parse_staging_stamp(text).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    Ok(LoadedStamp { stamp, bytes, dir })
}

pub fn hash_file(path: &Path) -> Result<[u8; 32], String> {
    let mut file = fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| format!("read {}: {e}", path.display()))?;
    Ok(*hasher.finalize().as_bytes())
}

/// One worker-start re-check: the files beside the stamp still hash to what
/// the stamp recorded. Returns `(check name, ok, detail)` rows; the caller
/// turns them into preflight `CheckResult`s. A missing recorded hash is a
/// failure (the stamp must describe the staged files).
pub fn verify_staged_files(loaded: &LoadedStamp) -> Vec<(&'static str, bool, String)> {
    let mut rows = Vec::new();
    let expect = |name: &'static str, file: &str, want: Option<[u8; 32]>| {
        let path = loaded.dir.join(file);
        match (want, hash_file(&path)) {
            (Some(want), Ok(got)) => (
                name,
                want == got,
                if want == got {
                    "hash matches stamp".to_owned()
                } else {
                    "hash differs from stamp".to_owned()
                },
            ),
            (Some(_), Err(e)) => (name, false, e),
            (None, _) => (
                name,
                false,
                "stamp records no hash for this file".to_owned(),
            ),
        }
    };
    let s = &loaded.stamp;
    rows.push(expect("image.bzImage", "bzImage", s.staged_bzimage_blake3));
    rows.push(expect(
        "image.initramfs.cpio",
        "initramfs.cpio",
        s.staged_initramfs_cpio_blake3,
    ));
    rows.push((
        "image.manifest_kernel",
        s.staged_bzimage_blake3 == Some(s.manifest_kernel_blake3),
        "manifest kernel hash == staged bzImage hash".to_owned(),
    ));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn hex(bytes: &[u8; 32]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn corpus_text(emu: Option<&str>) -> String {
        let mut s = format!(
            "# comment\nname=m9_linux_post_ready\nbzimage_blake3={}\ninitramfs_blake3={}\nbase_image_blake3={}\ngame_image_blake3={}\nend_icount=5\n",
            hex(&h(1)),
            hex(&h(2)),
            hex(&h(3)),
            hex(&h(4))
        );
        if let Some(emu) = emu {
            s.push_str(&format!("emu_version={emu}\n"));
        }
        s
    }

    fn staged() -> StagedArtifactHashes {
        StagedArtifactHashes {
            bzimage: h(1),
            initramfs: h(2),
            base_image: h(3),
            game_image: h(4),
        }
    }

    fn stamp_text() -> String {
        format!(
            "bundle_version=0.2.0\nmanifest_git_rev=abc123\nmanifest_emu_version=refwork-emu 0.2.3\nmanifest_kernel_blake3={}\nmanifest_initramfs_zst_blake3={}\nstaged_bzimage_blake3={}\nstaged_initramfs_cpio_blake3={}\nstaged_initramfs_zst_blake3={}\nstaged_at=2026-09-16T00:00:00Z\nstaged_by_host=box\n",
            hex(&h(1)),
            hex(&h(9)),
            hex(&h(1)),
            hex(&h(2)),
            hex(&h(9))
        )
    }

    #[test]
    fn parse_corpus_pins_reads_four_hashes_and_ignores_other_keys() {
        let pins = parse_corpus_pins(&corpus_text(None)).unwrap();
        assert_eq!(pins.bzimage, h(1));
        assert_eq!(pins.initramfs, h(2));
        assert_eq!(pins.base_image, h(3));
        assert_eq!(pins.game_image, h(4));
        assert_eq!(pins.emu_version, None);
        let pins = parse_corpus_pins(&corpus_text(Some("refwork-emu 0.2.3"))).unwrap();
        assert_eq!(pins.emu_version.as_deref(), Some("refwork-emu 0.2.3"));
    }

    #[test]
    fn parse_corpus_pins_rejects_bad_hex_and_missing_key() {
        let bad = corpus_text(None).replace(&hex(&h(2)), "zz");
        let err = parse_corpus_pins(&bad).unwrap_err();
        assert!(err.contains("initramfs_blake3"), "{err}");
        let missing = corpus_text(None)
            .lines()
            .filter(|l| !l.starts_with("game_image_blake3"))
            .collect::<Vec<_>>()
            .join("\n");
        let err = parse_corpus_pins(&missing).unwrap_err();
        assert!(err.contains("game_image_blake3: missing"), "{err}");
        assert!(parse_kv("novalue\n").is_err());
        assert!(parse_kv("a=1\na=2\n").is_err());
    }

    #[test]
    fn parse_staging_stamp_reads_identity_and_optional_hashes() {
        let stamp = parse_staging_stamp(&stamp_text()).unwrap();
        assert_eq!(stamp.bundle_version, "0.2.0");
        assert_eq!(stamp.manifest_emu_version, "refwork-emu 0.2.3");
        assert_eq!(stamp.manifest_kernel_blake3, h(1));
        assert_eq!(stamp.staged_initramfs_cpio_blake3, Some(h(2)));
        let minimal = "bundle_version=0.2.0\nmanifest_git_rev=x\nmanifest_emu_version=e\nmanifest_kernel_blake3=".to_owned() + &hex(&h(1)) + "\n";
        let stamp = parse_staging_stamp(&minimal).unwrap();
        assert_eq!(stamp.staged_bzimage_blake3, None);
        let err = parse_staging_stamp("bundle_version=0.2.0\n").unwrap_err();
        assert!(err.contains("manifest_git_rev: missing"), "{err}");
    }

    #[test]
    fn check_image_identity_matches_when_all_equal() {
        let pins = parse_corpus_pins(&corpus_text(Some("refwork-emu 0.2.3"))).unwrap();
        let stamp = parse_staging_stamp(&stamp_text()).unwrap();
        assert_eq!(
            check_image_identity(&pins, &staged(), Some(&stamp)),
            ImageIdentityVerdict::Match
        );
        assert!(check_image_identity(&pins, &staged(), None).is_match());
    }

    #[test]
    fn check_image_identity_lists_only_differing_keys() {
        let pins = parse_corpus_pins(&corpus_text(None)).unwrap();
        let mut s = staged();
        s.initramfs = h(7);
        assert_eq!(
            check_image_identity(&pins, &s, None),
            ImageIdentityVerdict::Mismatch(vec!["initramfs_blake3"])
        );
    }

    #[test]
    fn check_image_identity_with_stamp_requires_manifest_kernel_hash() {
        let pins = parse_corpus_pins(&corpus_text(Some("refwork-emu 0.2.3"))).unwrap();
        let mut stamp = parse_staging_stamp(&stamp_text()).unwrap();
        stamp.manifest_kernel_blake3 = h(8);
        assert_eq!(
            check_image_identity(&pins, &staged(), Some(&stamp)),
            ImageIdentityVerdict::Mismatch(vec!["manifest_kernel_blake3"])
        );
        let mut stamp = parse_staging_stamp(&stamp_text()).unwrap();
        stamp.staged_initramfs_zst_blake3 = Some(h(10));
        assert_eq!(
            check_image_identity(&pins, &staged(), Some(&stamp)),
            ImageIdentityVerdict::Mismatch(vec!["manifest_initramfs_zst_blake3"])
        );
    }

    #[test]
    fn emu_version_mismatch_is_reported_by_name() {
        // Pre-epoch manifest (no key) vs stamped bundle: exactly the
        // re-baseline situation, reported by key name only.
        let pins = parse_corpus_pins(&corpus_text(None)).unwrap();
        let stamp = parse_staging_stamp(&stamp_text()).unwrap();
        let verdict = check_image_identity(&pins, &staged(), Some(&stamp));
        assert_eq!(verdict, ImageIdentityVerdict::Mismatch(vec!["emu_version"]));
        assert_eq!(verdict.to_string(), "mismatch(emu_version)");
        let pins = parse_corpus_pins(&corpus_text(Some("refwork-emu 0.1.0"))).unwrap();
        assert_eq!(
            check_image_identity(&pins, &staged(), Some(&stamp)).mismatch_keys(),
            ["emu_version"]
        );
    }

    #[test]
    fn image_identity_string_is_version_at_stamp_hash() {
        let text = stamp_text();
        let stamp = parse_staging_stamp(&text).unwrap();
        let id = image_identity_string(&stamp, text.as_bytes());
        assert!(id.starts_with("0.2.0@"));
        assert_eq!(id.len(), "0.2.0@".len() + 64);
    }

    #[test]
    fn verify_staged_files_detects_tampering() {
        let dir = tempfile::TempDir::new().unwrap();
        let bz = b"kernel bytes";
        let ir = b"initramfs bytes";
        fs::write(dir.path().join("bzImage"), bz).unwrap();
        fs::write(dir.path().join("initramfs.cpio"), ir).unwrap();
        let bz_hex = blake3::hash(bz).to_hex().to_string();
        let ir_hex = blake3::hash(ir).to_hex().to_string();
        let text = format!(
            "bundle_version=0.2.0\nmanifest_git_rev=x\nmanifest_emu_version=refwork-emu 0.2.3\nmanifest_kernel_blake3={bz_hex}\nstaged_bzimage_blake3={bz_hex}\nstaged_initramfs_cpio_blake3={ir_hex}\n"
        );
        let stamp_path = dir.path().join("staging-stamp.txt");
        fs::write(&stamp_path, &text).unwrap();
        let loaded = load_staging_stamp(&stamp_path).unwrap();
        assert!(verify_staged_files(&loaded).iter().all(|(_, ok, _)| *ok));
        assert_eq!(
            loaded.identity(),
            image_identity_string(&loaded.stamp, text.as_bytes())
        );

        fs::write(dir.path().join("initramfs.cpio"), b"tampered").unwrap();
        let rows = verify_staged_files(&loaded);
        let bad: Vec<_> = rows
            .iter()
            .filter(|(_, ok, _)| !ok)
            .map(|(n, _, _)| *n)
            .collect();
        assert_eq!(bad, ["image.initramfs.cpio"]);

        fs::remove_file(dir.path().join("bzImage")).unwrap();
        let rows = verify_staged_files(&loaded);
        assert!(rows.iter().any(|(n, ok, _)| *n == "image.bzImage" && !ok));
        assert!(load_staging_stamp(&dir.path().join("nope.txt")).is_err());
    }
}
