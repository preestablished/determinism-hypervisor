//! Snapshot comparison helpers for VerifyReplay bisection diagnostics.
//!
//! This module deliberately does not run replay probes. It consumes two
//! snapshot refs, fetches their stored artifacts, and produces the compact
//! diagnostic payloads later VerifyReplay bisection code can attach to a
//! refined divergence.

use dh_snapshot::dhsnap::{tag, Container};
use dh_vmm::vcpu_state::{self, VcpuState};
use serde::{Deserialize, Serialize};
use snapstore_client::blocking::SnapstoreClient;
use snapstore_manifest::Manifest;
use snapstore_types::{PageHash, SnapshotRef};
use std::fmt;

pub const DIFF_PAGE_LIMIT: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotSide {
    Expected,
    Actual,
}

impl fmt::Display for SnapshotSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Expected => f.write_str("expected"),
            Self::Actual => f.write_str("actual"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegDiff {
    pub name: String,
    pub expected: Vec<u8>,
    pub actual: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotComparison {
    pub reg_diffs: Vec<RegDiff>,
    pub reg_diff: Vec<u8>,
    pub diff_page_idx: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotComparisonError {
    Store {
        side: SnapshotSide,
        op: &'static str,
        reason: String,
    },
    Manifest {
        side: SnapshotSide,
        reason: String,
    },
    UnsupportedDeviceBlob {
        side: SnapshotSide,
        format: u32,
        zstd: bool,
    },
    Dhsnap {
        side: SnapshotSide,
        reason: String,
    },
    MissingSection {
        side: SnapshotSide,
        tag: [u8; 4],
    },
    Vcpu {
        side: SnapshotSide,
        reason: String,
    },
    RegDiffEncode(String),
}

impl fmt::Display for SnapshotComparisonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store { side, op, reason } => {
                write!(f, "{side} snapshot {op} failed: {reason}")
            }
            Self::Manifest { side, reason } => {
                write!(f, "{side} snapshot manifest decode failed: {reason}")
            }
            Self::UnsupportedDeviceBlob { side, format, zstd } => {
                write!(
                    f,
                    "{side} snapshot device blob is not an uncompressed DHSNAP: format=0x{format:08x}, zstd={zstd}"
                )
            }
            Self::Dhsnap { side, reason } => {
                write!(f, "{side} snapshot DHSNAP decode failed: {reason}")
            }
            Self::MissingSection { side, tag } => {
                write!(
                    f,
                    "{side} snapshot DHSNAP is missing {} section",
                    tag_name(*tag)
                )
            }
            Self::Vcpu { side, reason } => {
                write!(f, "{side} snapshot VCPU decode failed: {reason}")
            }
            Self::RegDiffEncode(reason) => write!(f, "reg_diff postcard encode failed: {reason}"),
        }
    }
}

impl std::error::Error for SnapshotComparisonError {}

pub trait SnapshotComparisonStore {
    type Error: fmt::Display;

    fn get_snapshot_container(&self, snapshot_ref: SnapshotRef) -> Result<Vec<u8>, Self::Error>;

    fn resolve_page_hashes(
        &self,
        snapshot_ref: SnapshotRef,
    ) -> Result<Vec<(u64, PageHash)>, Self::Error>;
}

impl SnapshotComparisonStore for SnapstoreClient {
    type Error = String;

    fn get_snapshot_container(&self, snapshot_ref: SnapshotRef) -> Result<Vec<u8>, Self::Error> {
        self.get_snapshot(snapshot_ref).map_err(|e| e.to_string())
    }

    fn resolve_page_hashes(
        &self,
        snapshot_ref: SnapshotRef,
    ) -> Result<Vec<(u64, PageHash)>, Self::Error> {
        self.resolve_pages(snapshot_ref, None, true)
            .map_err(|e| e.to_string())
            .map(|pages| {
                pages
                    .into_iter()
                    .map(|(page_index, page_hash, _payload)| (page_index, page_hash))
                    .collect()
            })
    }
}

pub fn compare_snapshots<S>(
    store: &S,
    expected_ref: SnapshotRef,
    actual_ref: SnapshotRef,
) -> Result<SnapshotComparison, SnapshotComparisonError>
where
    S: SnapshotComparisonStore,
{
    let expected_container = store
        .get_snapshot_container(expected_ref.clone())
        .map_err(|e| SnapshotComparisonError::Store {
            side: SnapshotSide::Expected,
            op: "get_snapshot",
            reason: e.to_string(),
        })?;
    let actual_container = store
        .get_snapshot_container(actual_ref.clone())
        .map_err(|e| SnapshotComparisonError::Store {
            side: SnapshotSide::Actual,
            op: "get_snapshot",
            reason: e.to_string(),
        })?;

    let expected_vcpu = decode_vcpu(&expected_container, SnapshotSide::Expected)?;
    let actual_vcpu = decode_vcpu(&actual_container, SnapshotSide::Actual)?;
    let reg_diffs = compare_vcpu(&expected_vcpu, &actual_vcpu);
    let reg_diff = postcard::to_allocvec(&reg_diffs)
        .map_err(|e| SnapshotComparisonError::RegDiffEncode(e.to_string()))?;

    let expected_pages =
        store
            .resolve_page_hashes(expected_ref)
            .map_err(|e| SnapshotComparisonError::Store {
                side: SnapshotSide::Expected,
                op: "resolve_pages(hashes_only=true)",
                reason: e.to_string(),
            })?;
    let actual_pages =
        store
            .resolve_page_hashes(actual_ref)
            .map_err(|e| SnapshotComparisonError::Store {
                side: SnapshotSide::Actual,
                op: "resolve_pages(hashes_only=true)",
                reason: e.to_string(),
            })?;

    Ok(SnapshotComparison {
        reg_diffs,
        reg_diff,
        diff_page_idx: first_diff_page_indices(expected_pages, actual_pages),
    })
}

fn decode_vcpu(container: &[u8], side: SnapshotSide) -> Result<VcpuState, SnapshotComparisonError> {
    let manifest = Manifest::decode(container).map_err(|e| SnapshotComparisonError::Manifest {
        side,
        reason: e.to_string(),
    })?;
    let blob = &manifest.device_blob;
    if blob.format != crate::snapshot_engine::DEVICE_BLOB_FORMAT_DHSNAP || blob.zstd {
        return Err(SnapshotComparisonError::UnsupportedDeviceBlob {
            side,
            format: blob.format,
            zstd: blob.zstd,
        });
    }
    let dhsnap = Container::parse(&blob.bytes).map_err(|e| SnapshotComparisonError::Dhsnap {
        side,
        reason: format!("{e:?}"),
    })?;
    let vcpu = dhsnap
        .get(tag::VCPU)
        .ok_or(SnapshotComparisonError::MissingSection {
            side,
            tag: tag::VCPU,
        })?;
    vcpu_state::decode_section(vcpu.contents, vcpu.sec_version).map_err(|e| {
        SnapshotComparisonError::Vcpu {
            side,
            reason: format!("{e:?}"),
        }
    })
}

fn compare_vcpu(expected: &VcpuState, actual: &VcpuState) -> Vec<RegDiff> {
    let mut diffs = Vec::new();
    let e = &expected.regs;
    let a = &actual.regs;

    push_u64_diff(&mut diffs, "rax", e.rax, a.rax);
    push_u64_diff(&mut diffs, "rbx", e.rbx, a.rbx);
    push_u64_diff(&mut diffs, "rcx", e.rcx, a.rcx);
    push_u64_diff(&mut diffs, "rdx", e.rdx, a.rdx);
    push_u64_diff(&mut diffs, "rsi", e.rsi, a.rsi);
    push_u64_diff(&mut diffs, "rdi", e.rdi, a.rdi);
    push_u64_diff(&mut diffs, "rsp", e.rsp, a.rsp);
    push_u64_diff(&mut diffs, "rbp", e.rbp, a.rbp);
    push_u64_diff(&mut diffs, "r8", e.r8, a.r8);
    push_u64_diff(&mut diffs, "r9", e.r9, a.r9);
    push_u64_diff(&mut diffs, "r10", e.r10, a.r10);
    push_u64_diff(&mut diffs, "r11", e.r11, a.r11);
    push_u64_diff(&mut diffs, "r12", e.r12, a.r12);
    push_u64_diff(&mut diffs, "r13", e.r13, a.r13);
    push_u64_diff(&mut diffs, "r14", e.r14, a.r14);
    push_u64_diff(&mut diffs, "r15", e.r15, a.r15);
    push_u64_diff(&mut diffs, "rip", e.rip, a.rip);
    push_u64_diff(&mut diffs, "rflags", e.rflags, a.rflags);

    let mut actual_without_gpr_diffs = actual.clone();
    actual_without_gpr_diffs.regs = expected.regs;
    if expected != &actual_without_gpr_diffs {
        diffs.push(RegDiff {
            name: "vcpu_non_gpr".into(),
            expected: vcpu_state::encode_section(expected),
            actual: vcpu_state::encode_section(actual),
        });
    }

    diffs
}

fn push_u64_diff(diffs: &mut Vec<RegDiff>, name: &'static str, expected: u64, actual: u64) {
    if expected != actual {
        diffs.push(RegDiff {
            name: name.into(),
            expected: expected.to_le_bytes().to_vec(),
            actual: actual.to_le_bytes().to_vec(),
        });
    }
}

fn first_diff_page_indices(
    mut expected: Vec<(u64, PageHash)>,
    mut actual: Vec<(u64, PageHash)>,
) -> Vec<u64> {
    expected.sort_by_key(|(idx, _)| *idx);
    actual.sort_by_key(|(idx, _)| *idx);

    let mut diffs = Vec::new();
    let mut e = 0usize;
    let mut a = 0usize;
    while diffs.len() < DIFF_PAGE_LIMIT && (e < expected.len() || a < actual.len()) {
        match (expected.get(e), actual.get(a)) {
            (Some((ei, eh)), Some((ai, ah))) if ei == ai => {
                if eh != ah {
                    diffs.push(*ei);
                }
                e += 1;
                a += 1;
            }
            (Some((ei, _)), Some((ai, _))) if ei < ai => {
                diffs.push(*ei);
                e += 1;
            }
            (Some(_), Some((ai, _))) => {
                diffs.push(*ai);
                a += 1;
            }
            (Some((ei, _)), None) => {
                diffs.push(*ei);
                e += 1;
            }
            (None, Some((ai, _))) => {
                diffs.push(*ai);
                a += 1;
            }
            (None, None) => break,
        }
    }
    diffs
}

fn tag_name(tag: [u8; 4]) -> String {
    String::from_utf8_lossy(&tag).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dh_snapshot::dhsnap::{tag, ContainerWriter};
    use dh_vmm::vcpu_state::{RESTORE_MSR_LIST, VCPU_SECTION_VERSION, XSAVE_AREA_LEN};
    use kvm_bindings::kvm_msr_entry;
    use snapstore_manifest::{DeviceBlob, Manifest, ManifestEntry};
    use snapstore_types::PAGE_SIZE;
    use std::collections::BTreeMap;

    #[derive(Clone)]
    struct SnapshotFixture {
        snapshot_ref: SnapshotRef,
        container: Vec<u8>,
        pages: Vec<(u64, PageHash)>,
    }

    #[derive(Default)]
    struct FakeStore {
        snapshots: BTreeMap<[u8; 32], SnapshotFixture>,
    }

    impl FakeStore {
        fn insert(&mut self, fixture: SnapshotFixture) {
            self.snapshots
                .insert(fixture.snapshot_ref.to_bytes(), fixture);
        }
    }

    impl SnapshotComparisonStore for FakeStore {
        type Error = String;

        fn get_snapshot_container(
            &self,
            snapshot_ref: SnapshotRef,
        ) -> Result<Vec<u8>, Self::Error> {
            self.snapshots
                .get(&snapshot_ref.to_bytes())
                .map(|fixture| fixture.container.clone())
                .ok_or_else(|| "missing snapshot".into())
        }

        fn resolve_page_hashes(
            &self,
            snapshot_ref: SnapshotRef,
        ) -> Result<Vec<(u64, PageHash)>, Self::Error> {
            self.snapshots
                .get(&snapshot_ref.to_bytes())
                .map(|fixture| fixture.pages.clone())
                .ok_or_else(|| "missing snapshot".into())
        }
    }

    fn synthetic_state() -> VcpuState {
        let mut xsave = vec![0u8; XSAVE_AREA_LEN];
        xsave[24..28].copy_from_slice(&0x1F80u32.to_le_bytes());
        let mut state = VcpuState {
            regs: Default::default(),
            sregs: Default::default(),
            fpu: Default::default(),
            xsave,
            xcrs: Default::default(),
            msrs: RESTORE_MSR_LIST
                .iter()
                .enumerate()
                .map(|(i, &index)| {
                    let mut entry = kvm_msr_entry::default();
                    entry.index = index;
                    entry.data = 0x1000 + i as u64;
                    entry
                })
                .collect(),
            events: Default::default(),
            dbg: Default::default(),
        };
        state.regs.rip = 0x1000;
        state.regs.rax = 0xAA55;
        state.regs.rflags = 2;
        state.sregs.cr0 = 0x11;
        state.sregs.cr4 = 0x200;
        state.sregs.efer = 0x500;
        state.dbg.dr7 = 0x400;
        state
    }

    fn page_hash(bytes: &[u8]) -> PageHash {
        PageHash::from_bytes(*blake3::hash(bytes).as_bytes())
    }

    fn page(byte: u8) -> Vec<u8> {
        vec![byte; PAGE_SIZE]
    }

    fn fixture_with_vcpu(state: &VcpuState, pages: Vec<Vec<u8>>) -> SnapshotFixture {
        let mut writer = ContainerWriter::new();
        writer
            .push_section(
                tag::VCPU,
                VCPU_SECTION_VERSION,
                &vcpu_state::encode_section(state),
            )
            .unwrap();
        fixture_with_dhsnap(writer.finish(), pages)
    }

    fn fixture_without_vcpu(pages: Vec<Vec<u8>>) -> SnapshotFixture {
        let mut writer = ContainerWriter::new();
        writer.push_section(tag::MCFG, 1, b"config").unwrap();
        fixture_with_dhsnap(writer.finish(), pages)
    }

    fn fixture_with_dhsnap(dhsnap: Vec<u8>, pages: Vec<Vec<u8>>) -> SnapshotFixture {
        let page_hashes: Vec<(u64, PageHash)> = pages
            .iter()
            .enumerate()
            .map(|(idx, bytes)| (idx as u64, page_hash(bytes)))
            .collect();
        let entries: Vec<ManifestEntry> = page_hashes
            .iter()
            .map(|(page_index, page_hash)| ManifestEntry {
                page_index: *page_index,
                page_hash: *page_hash,
            })
            .collect();
        let manifest = Manifest::new_full(
            (pages.len() * PAGE_SIZE) as u64,
            entries,
            DeviceBlob {
                format: crate::snapshot_engine::DEVICE_BLOB_FORMAT_DHSNAP,
                zstd: false,
                raw_len: dhsnap.len() as u64,
                bytes: dhsnap,
            },
        )
        .unwrap();
        let container = manifest.encode();
        SnapshotFixture {
            snapshot_ref: Manifest::snapshot_ref(&container),
            container,
            pages: page_hashes,
        }
    }

    fn decode_reg_diff(bytes: &[u8]) -> Vec<RegDiff> {
        postcard::from_bytes(bytes).unwrap()
    }

    #[test]
    fn matching_snapshots_have_empty_diagnostics() {
        let state = synthetic_state();
        let fixture = fixture_with_vcpu(&state, vec![page(0), page(1)]);
        let mut store = FakeStore::default();
        store.insert(fixture.clone());

        let comparison =
            compare_snapshots(&store, fixture.snapshot_ref.clone(), fixture.snapshot_ref).unwrap();

        assert!(comparison.reg_diffs.is_empty());
        assert!(decode_reg_diff(&comparison.reg_diff).is_empty());
        assert!(comparison.diff_page_idx.is_empty());
    }

    #[test]
    fn rip_mismatch_produces_postcard_reg_diff() {
        let expected = synthetic_state();
        let mut actual = expected.clone();
        actual.regs.rip = 0x2000;
        let expected = fixture_with_vcpu(&expected, vec![page(0)]);
        let actual = fixture_with_vcpu(&actual, vec![page(0)]);
        let mut store = FakeStore::default();
        store.insert(expected.clone());
        store.insert(actual.clone());

        let comparison = compare_snapshots(
            &store,
            expected.snapshot_ref.clone(),
            actual.snapshot_ref.clone(),
        )
        .unwrap();

        let decoded = decode_reg_diff(&comparison.reg_diff);
        assert_eq!(decoded, comparison.reg_diffs);
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].name, "rip");
        assert_eq!(decoded[0].expected, 0x1000u64.to_le_bytes());
        assert_eq!(decoded[0].actual, 0x2000u64.to_le_bytes());
        assert!(comparison.diff_page_idx.is_empty());
    }

    #[test]
    fn non_gpr_vcpu_mismatch_still_produces_reg_diff_payload() {
        let expected = synthetic_state();
        let mut actual = expected.clone();
        actual.sregs.cr3 = 0x5555_0000;
        let expected = fixture_with_vcpu(&expected, vec![page(0)]);
        let actual = fixture_with_vcpu(&actual, vec![page(0)]);
        let mut store = FakeStore::default();
        store.insert(expected.clone());
        store.insert(actual.clone());

        let comparison =
            compare_snapshots(&store, expected.snapshot_ref, actual.snapshot_ref).unwrap();

        let decoded = decode_reg_diff(&comparison.reg_diff);
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].name, "vcpu_non_gpr");
        assert!(decoded[0].expected.len() > 4096);
        assert_eq!(decoded[0].expected.len(), decoded[0].actual.len());
    }

    #[test]
    fn combined_gpr_and_non_gpr_mismatch_keeps_actual_vcpu_payload() {
        let expected_state = synthetic_state();
        let mut actual_state = expected_state.clone();
        actual_state.regs.rip = 0x2000;
        actual_state.sregs.cr3 = 0x5555_0000;
        let expected_vcpu = vcpu_state::encode_section(&expected_state);
        let actual_vcpu = vcpu_state::encode_section(&actual_state);
        let expected = fixture_with_vcpu(&expected_state, vec![page(0)]);
        let actual = fixture_with_vcpu(&actual_state, vec![page(0)]);
        let mut store = FakeStore::default();
        store.insert(expected.clone());
        store.insert(actual.clone());

        let comparison =
            compare_snapshots(&store, expected.snapshot_ref, actual.snapshot_ref).unwrap();

        let decoded = decode_reg_diff(&comparison.reg_diff);
        assert_eq!(decoded.len(), 2);
        assert!(decoded.iter().any(|diff| diff.name == "rip"));
        let non_gpr = decoded
            .iter()
            .find(|diff| diff.name == "vcpu_non_gpr")
            .unwrap();
        assert_eq!(non_gpr.expected, expected_vcpu);
        assert_eq!(non_gpr.actual, actual_vcpu);
    }

    #[test]
    fn page_hash_mismatch_reports_page_index() {
        let state = synthetic_state();
        let expected = fixture_with_vcpu(&state, vec![page(0), page(1), page(2)]);
        let actual = fixture_with_vcpu(&state, vec![page(0), page(9), page(2)]);
        let mut store = FakeStore::default();
        store.insert(expected.clone());
        store.insert(actual.clone());

        let comparison =
            compare_snapshots(&store, expected.snapshot_ref, actual.snapshot_ref).unwrap();

        assert!(comparison.reg_diffs.is_empty());
        assert_eq!(comparison.diff_page_idx, vec![1]);
    }

    #[test]
    fn page_hash_mismatches_are_limited_to_first_64_indices() {
        let state = synthetic_state();
        let expected_pages: Vec<Vec<u8>> = (0..70).map(|_| page(0)).collect();
        let actual_pages: Vec<Vec<u8>> = (0..70).map(|idx| page(idx as u8 + 1)).collect();
        let expected = fixture_with_vcpu(&state, expected_pages);
        let actual = fixture_with_vcpu(&state, actual_pages);
        let mut store = FakeStore::default();
        store.insert(expected.clone());
        store.insert(actual.clone());

        let comparison =
            compare_snapshots(&store, expected.snapshot_ref, actual.snapshot_ref).unwrap();

        assert_eq!(comparison.diff_page_idx.len(), DIFF_PAGE_LIMIT);
        assert_eq!(comparison.diff_page_idx[0], 0);
        assert_eq!(comparison.diff_page_idx[63], 63);
    }

    #[test]
    fn missing_snapshot_ref_is_loud() {
        let state = synthetic_state();
        let expected = fixture_with_vcpu(&state, vec![page(0)]);
        let mut store = FakeStore::default();
        store.insert(expected.clone());

        let err = compare_snapshots(
            &store,
            expected.snapshot_ref,
            SnapshotRef::from_bytes([0xEE; 32]),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            SnapshotComparisonError::Store {
                side: SnapshotSide::Actual,
                op: "get_snapshot",
                ..
            }
        ));
    }

    #[test]
    fn missing_vcpu_section_is_loud() {
        let state = synthetic_state();
        let expected = fixture_without_vcpu(vec![page(0)]);
        let actual = fixture_with_vcpu(&state, vec![page(0)]);
        let mut store = FakeStore::default();
        store.insert(expected.clone());
        store.insert(actual.clone());

        let err =
            compare_snapshots(&store, expected.snapshot_ref, actual.snapshot_ref).unwrap_err();

        assert!(matches!(
            err,
            SnapshotComparisonError::MissingSection {
                side: SnapshotSide::Expected,
                tag: dh_snapshot::dhsnap::tag::VCPU,
            }
        ));
    }
}
