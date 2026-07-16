# Positive Notes

### P-1 — Exact §6.5 register conformance

Register offsets and widths match ARCHITECTURE.md §6.5 precisely: SECTOR 0x08
(8B), BUF_GPA 0x10 (8B), COUNT 0x18 (4B sectors), CMD 0x1C (4B WO, 1/2/3),
STATUS 0x20 (4B RO). The registers tile contiguously and every one is naturally
aligned, so the bus `check_access` (`bus.rs:73-83`) never rejects a legitimate
access. CMD-write-only and STATUS-read-only are enforced in `mmio_read`/
`mmio_write` (`blk.rs:217-241`) and verified by `registers_echo_and_status_is_
read_only`.

### P-2 — Overflow reasoning is correct and the apparent `sector * 512` hazard is provably safe

`request_range` (`blk.rs:129-144`) checks `count != 0`, uses `checked_add` for
`sector + count`, and bounds `end_sector <= capacity_sectors()`. Since
`capacity = len/512`, `sector <= end_sector <= len/512` implies
`sector * 512 <= len`, so the unchecked `self.sector * SECTOR_SIZE` cannot
overflow. The `gpa += take` advances (`blk.rs:165-166, 200-202`) likewise cannot
overflow: each advance follows a *successful* in-range `ctx.mem` access, so
`gpa + take <= mem.len() <= usize::MAX < u64::MAX`. The `u64::MAX` sector edge
case is tested (`bad_requests_and_unknown_cmds_set_status`).

### P-3 — CoW base immutability proven structurally in tests

`VecBase` wraps `Rc<Vec<u8>>` (`blk.rs:305`) so the device *cannot* hold a
mutable reference to the base — the type system proves CoW never writes the
base. `FileBase` opens `O_RDONLY` (`blkfile.rs:24`) so the fd cannot write. The
§6.5 acceptance test (`base_file_bytes_and_mtime_unchanged_after_writes`)
checks both bytes and mtime are unchanged after device writes. RMW-from-base is
proven by `rmw_preserves_cluster_neighbors`.

### P-4 — Snapshot determinism is order-free and directly tested

The snapshot sorts cluster indices (`blk.rs:248-251`) so HashMap iteration order
never leaks into the bytes. `snapshot_is_sorted_deterministic_and_roundtrips`
builds two devices that dirty the same clusters in *opposite* order and asserts
their overlay serialization is byte-identical — exactly the right test for the
"pure function of device state" requirement. Per-cluster blake3 digests guard
against bit-rot/truncation, and the tamper/wrong-version/truncated-input refusal
paths are all tested.

### P-5 — Deny-list discipline respected; backend split is correct

`blk.rs` names no host I/O / time / randomness tokens, and the
`no_host_ambient_authority` grep gate passes. The production file backend
correctly lives in `dh-vmm` (which has no deny-list gate) behind the `BlockBase`
seam, with a clear doc comment (`blkfile.rs:7-9`) explaining *why* it lives
there. Clippy `disallowed_types`/`disallowed_methods` are `deny` and the crate
compiles clean.

### P-6 — Host-fault channel design is well-reasoned

Routing base-read failures to a distinct `STATUS_HOST_IO` plus a non-serialized
`host_io_errors` counter (a metric, correctly kept out of the snapshot at
`blk.rs:243-258` so the snapshot stays a pure function of *device* state) is the
right separation. The doc comment (`blk.rs:49-55`) articulates exactly why a
host I/O error is slot-fatal and cannot be replayed. `DeadBase` tests both the
read and write host-fault paths and asserts the counter increments.

### P-7 — Clear, normative-quality documentation throughout

Both modules open with substantial doc comments that tie each design choice back
to §6.5 and the determinism mandate (one-deep regs, synchronous completion,
overlay-first reads, sorted snapshot). The `device_id 0x0005` is unique among
pv-clock 0x0002 / pv-pad 0x0003 / pv-entropy 0x0004, and the detchannel id lives
in a different namespace with no MMIO-window collision. This is the kind of code
that is easy for the next reviewer to reason about.
