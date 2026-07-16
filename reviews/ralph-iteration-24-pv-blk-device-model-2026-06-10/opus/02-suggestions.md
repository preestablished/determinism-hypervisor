# Suggestions

### S-1 — Restoring an in-flight `status` is technically meaningful but worth a one-line note

`blk.rs:243-291`. The snapshot latches `status` and `restore` writes it back.
With one-deep registers and synchronous completion, there is no in-flight
request at a snapshot boundary — the device is always quiescent (the CMD write
emulation runs to completion before the VM exit returns). So `status` always
reflects the *last completed* request, which is the correct thing to persist
(a guest that snapshots, restores, then reads STATUS sees its last result). The
SECTOR/BUF_GPA/COUNT latches likewise are last-written values. This is correct;
a one-line comment "registers are latched values of the last completed request;
the device is always quiescent at a snapshot boundary (synchronous completion)"
would make the intent obvious and forestall a future reviewer worrying about
in-flight state.

### S-2 — `restore` does not validate `status`/`count`/register values against the device's own `base`

`blk.rs:269-289`. `restore` digest-checks each cluster (good) and validates the
total length, but it accepts any `sector`, `count`, `status`, and any cluster
`idx` — including a cluster index that lies past the current base's
`capacity_sectors()`. Because the overlay is consulted overlay-first on read
(`blk.rs:157`), a restored cluster with an out-of-range idx would be dead state
(no request can address it, since `request_range` bounds by capacity) — so it is
harmless, just wasted bytes. But restoring a snapshot taken against a
*different* base (different length) is silently accepted. The MachineConfig
content hash is the real guard for base identity, and DHSNAP framing owns
section identity, so this is defensible as out-of-scope. Consider at least a
debug-assert or doc note that "restore assumes the same base as the snapshot;
base identity is enforced by MachineConfig hash, not here."

### S-3 — `STATUS_HOST_IO = 0xFE` collides conceptually with no other code but is an odd sentinel

`blk.rs:55`. 0xFE is fine and distinct from 0/1/2. Minor: a short comment noting
"0xFE chosen to sit clearly outside the small OK/BAD/FAULT range and to be an
obvious 'host' sentinel" would help. Non-blocking.

### S-4 — `FileBase::read_at` short-read path: `read_exact_at` on a racing truncation

`blkfile.rs:45-57`. The function computes `take = min(len - offset, buf.len())`
using the `len` captured at `open()` time, then calls `read_exact_at`. If the
underlying file were truncated *after* open (it is `O_RDONLY`, but another
process could truncate the inode), `read_exact_at` would return
`UnexpectedEof`, correctly mapped to `BaseIoError` → `STATUS_HOST_IO`. Good —
this is the right failure mode (host fault, slot-fatal). Worth a one-line
comment that the cached `len` is trusted and a mid-run truncation surfaces as
`HOST_IO`, which is the intended slot-fatal path. The `usize::try_from(...)`
guard against a >usize length on a 32-bit host is correct defensive code even
though the project is x86_64-only.

### S-5 — Zero-fill-past-EOF contract is duplicated in three places; consider a shared default or a doc-test

`blk.rs:64-75` (trait doc), `blk.rs:311-319` (VecBase), `blkfile.rs:45-57`
(FileBase). The contract ("`buf.fill(0)` first, then fill `[..take]`") is
re-implemented identically in both impls. A future third impl could forget the
`buf.fill(0)`. Options: (a) provide it as documented boilerplate the impl must
follow (current state), or (b) add a `#[cfg(test)]` conformance helper that any
`BlockBase` impl can be run through (offset past EOF returns all-zeros, partial
tail zero-fills the remainder). The cross-EOF behavior is the single most
error-prone part of a new backend; a reusable conformance test would pay off.

### S-6 — `capacity_sectors` floors a trailing partial sector silently

`blk.rs:106-109`. `len_bytes / 512` correctly drops a partial trailing sector
(documented at `blk.rs:68-69`), and `blkfile.rs` test `eof` exercises the 2.5-
sector case (capacity floors to 2). Good. One observation: the RMW path in
`do_write` reads `cluster * CLUSTER_SIZE` for a *whole 64 KiB cluster*
(`blk.rs:187`), which for the last cluster reads well past `len_bytes` and
relies on zero-fill. That is correct given the contract, and the `eof` test
covers it — but it means the last cluster's overlay holds zero-filled bytes
beyond the real image tail. Since those sectors are not addressable (beyond
capacity), this is invisible. No change needed; calling it out for the record.

---

## NEEDS_DISCUSSION (architectural)

### D-1 — All-or-nothing fault semantics vs. partial application

Tied to I-1. The cleanest determinism story is for `MEM_FAULT` to be
all-or-nothing: pre-validate the full `[buf_gpa, buf_gpa + count*512)` range
against guest memory (and for writes, also that the read of guest RAM will
succeed) before mutating any guest RAM or overlay cluster. That removes the
"partial side effects must replay identically" reasoning entirely and matches a
DMA engine reporting a fault without committing a partial transfer. The cost is
a pre-pass over the range (cheap) or a two-phase write (stage into clusters,
commit on full success). Given the project's determinism-first posture, this is
worth a deliberate decision now rather than after the production guest-memory
backend lands and makes partial-fault behavior load-bearing. Recommend a short
design note / bead capturing the chosen semantics.
