//! dh-devices (ARCH §6): deterministic device models.
//!
//! Every device is a plain Rust state machine driven exclusively through
//! [`DetDevice`] with a [`ctx::DevCtx`]. No device may read host time, host
//! randomness, or do host I/O on the execution path. That rule is enforced
//! three ways: this crate's lint config (clippy `disallowed_types` /
//! `disallowed_methods`, see the workspace clippy.toml), the
//! `no_host_ambient_authority` source-grep test below, and code review.
#![forbid(unsafe_code)]
#![deny(clippy::disallowed_types, clippy::disallowed_methods)]

pub mod blk;
pub mod bus;
pub mod clock;
pub mod ctx;
pub mod detchannel;
pub mod entropy;
pub mod net;
pub mod pad;
pub mod serial;

pub use bus::{BusError, MmioBus, WINDOW_LEN};
pub use ctx::{DevCtx, EntropySource, GuestMem, IrqRequest, MemError};
pub use detchannel::{DetChannelHost, DetChannelMetrics};
pub use serial::DebugSerial;

pub const DEVICE_MODEL_COMPONENT: &str = "dh-devices";

/// Restore failed: section bytes malformed for this device/version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestoreError;

/// The device contract (ARCH §6).
///
/// Snapshot seam (deliberate Phase-1 shape): devices produce/consume their
/// raw DHSNAP *section contents*; the section framing (tag, sec_version,
/// len — API.md §4) is owned by `dh-snapshot`, which wraps these calls when
/// it lands. `device_id` doubles as the §6.1 MAGIC register value and
/// `section_version` as VERSION — both are served by the bus, read-only.
///
/// NOTE for dh-snapshot: the DHSNAP section tag is a 4-byte ASCII name from
/// its own section table (e.g. `CLKD`) — it is NOT derived from
/// `device_id`. The id↔tag mapping lives in dh-snapshot, one place.
pub trait DetDevice: Send {
    fn device_id(&self) -> u16;
    /// Version of this device's snapshot section layout (and the §6.1
    /// VERSION register value).
    fn section_version(&self) -> u16;
    /// `off` is window-relative, ≥ 0x08 (bus owns 0x00/0x04), pre-validated
    /// to 4/8 bytes naturally aligned. Unknown offsets must read as zeros —
    /// never host state.
    fn mmio_read(&mut self, off: u64, data: &mut [u8], ctx: &mut DevCtx);
    /// Same preconditions as `mmio_read`. Unknown offsets are ignored.
    fn mmio_write(&mut self, off: u64, data: &[u8], ctx: &mut DevCtx);
    /// Append this device's section contents (versioned by
    /// `section_version`). Must be a pure function of device state.
    fn snapshot(&self, out: &mut Vec<u8>);
    fn restore(&mut self, bytes: &[u8], sec_version: u16) -> Result<(), RestoreError>;
    /// Restore-engine downcast seam (ARCH §8.3). A device whose restore
    /// needs engine-supplied state that is NOT in its own DHSNAP section
    /// overrides this to expose its concrete type. Today that is exactly
    /// `PvClock`: its `vns_base` comes from the snapshot's TIME section
    /// (the new segment starts at icount 0, so base = absolute vns at the
    /// boundary) — by design CLKD never carries it. Default: no downcast.
    ///
    /// Override ONLY when the restore engine must reach the concrete type,
    /// and keep the override consistent with the `device_id` the engine
    /// matches on — the engine keys the downcast by id, so an override on
    /// an unrelated device is dead code at best and a misdirected downcast
    /// target at worst.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        None
    }
}

pub fn input_payload_digest(payload: &[u8]) -> [u8; 32] {
    dh_inputlog::payload_digest(payload)
}

#[cfg(test)]
mod deny_list_gate {
    /// The CI-grep half of the §6 deny-list gate: the execution path in
    /// this crate must never name host time, host randomness, network, or
    /// filesystem APIs. (The clippy half catches typed usage; this catches
    /// any spelling, including fully-qualified paths in new code.)
    #[test]
    fn no_host_ambient_authority() {
        let src_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let forbidden = [
            "std::time",
            "rand::",
            "std::net",
            "std::fs",
            "SystemTime",
            "Instant::now",
            // Host entropy: the seeded ChaCha20 stream is required (ARCH §5),
            // but OsRng/getrandom must never appear ("rand::" misses
            // "rand_core::OsRng"). getrandom is also absent from Cargo.lock;
            // this catches it returning via a feature bump.
            "OsRng",
            "getrandom",
            "thread_rng",
        ];
        let mut hits = Vec::new();
        for entry in std::fs::read_dir(src_dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let allowed_here = path.ends_with("lib.rs");
            for (lineno, line) in text.lines().enumerate() {
                if allowed_here {
                    continue; // this gate module is the one allowed mention site
                }
                for tok in forbidden {
                    if line.contains(tok) {
                        hits.push(format!("{}:{}: {tok}", path.display(), lineno + 1));
                    }
                }
            }
        }
        assert!(
            hits.is_empty(),
            "deny-listed host APIs found:\n{}",
            hits.join("\n")
        );
    }
}
