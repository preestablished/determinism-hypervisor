# Positive Notes

### P-1. Writer/reader NET_TX layout is byte-for-byte symmetric

`LogWriter::net_tx` (`dhilog.rs:238-249`) writes `len` to payload[0..4] and
`digest8` to payload[8..16] (bytes 4..8 zero-pad), then emits a 16-byte AUX
record. `reader.rs:192` decodes `len: u32at(0)`, `digest8: u64at(8)`, and the
validator at `reader.rs:537` enforces `payload.len() == 16` for `KIND_NET_TX`.
Identical to the established ENTROPY/SDK_EVENT convention. No drift.

### P-2. The TX/RX output-vs-input split is exactly right for determinism

TX buffers no frame; the doorbell logs only `(len, digest8)` and replay
re-derives the digest from the guest re-executing the same MMIO write. RX is an
input that only flows from canonical `NET_RX` records via `apply_net_rx`. This
is the correct shape for a deterministic hypervisor — and the module doc's
reasoning for why the NETL section can never have pending state ("there is no
queue") is genuinely airtight.

### P-3. `digest8` reuses the single shared BLAKE3 helper

`doorbell` calls `LogWriter::digest8(&frame)` (`net.rs:142`), the same
first-8-bytes-of-BLAKE3-LE helper (`dhilog.rs:138`) used by ENTROPY/SDK_EVENT.
No bespoke hashing, no chance of a per-device digest divergence.

### P-4. DEVICE_ID pinned to the snapshot map, and tested

`DEVICE_ID_PV_NET = 0x0007` matches `dhsnap.rs:93` (`0x0007 => tag::NETL`), the
next free id after debug-serial 0x0006, and `device_id_is_the_dhsnap_pinned_0x0007`
asserts both the constant and the trait method. The bead note's warning about
silent map divergence is directly addressed.

### P-5. Strong, adversarial test coverage of the failure paths

The 7 tests cover the documented error variants, not just the happy path:
zero-length / oversize / unmapped-buffer TX faults (all → STATUS_FAULT,
nothing logged), `apply_net_rx` rejections (over-cap, empty, unmapped), the
vector-enabled vs vector-disabled RX outcomes, the snapshot/restore identity
round-trip *plus* wrong-version and wrong-length rejection, and the
unknown-offset RAZ/WI behavior. This matches the research file's guidance to
reach every documented error variant.

### P-6. Snapshot/restore is register-only, length- and version-checked

`SECTION_LEN = 36` is computed from the field widths, `restore` rejects both a
mismatched `sec_version` and a wrong byte length before touching any field, and
the round-trip test asserts byte-identity. The deny-list gate scanning
`src/*.rs` via `read_dir` means `net.rs` is automatically covered for host
ambient authority — no manual allow-list edit needed.
