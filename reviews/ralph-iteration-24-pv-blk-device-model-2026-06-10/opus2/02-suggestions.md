# Suggestions — pv-blk

### S-1. `request_range` arithmetic: unchecked `*` is safe only via a non-local argument

`request_range` (blk.rs:135-150) returns:

```rust
Ok((self.sector * SECTOR_SIZE as u64, self.count as usize * SECTOR_SIZE))
```

Both multiplies are unchecked. They are currently safe, but the argument is non-obvious:

- `self.sector * 512`: guarded only indirectly. `end_sector = sector + count <= capacity_sectors() = len_bytes/512`. So `sector < len_bytes/512`, hence `sector*512 < len_bytes <= u64::MAX` — **no overflow, but only if `len_bytes` is honest.** `BlockBase` is a trait; a hostile or buggy impl returning `len_bytes()` near `u64::MAX` with `read_at` succeeding makes `capacity_sectors()` near `2^55`, and a request at `sector = 2^53, count = 1` passes the range check, then `sector*512` is fine (still < len_bytes) — so even hostile `len_bytes` cannot overflow this particular product. Good, but undocumented.
- `self.count as usize * SECTOR_SIZE`: `count` is `u32` (max `~4.29e9`), `* 512` = `~2.2e12`, fits `usize` on 64-bit comfortably. On 32-bit `usize` it would overflow — but this is an x86_64-only KVM project, so theoretical.

**Suggestion:** add a comment stating the overflow-safety invariant ("`sector*512 < len_bytes` by the range check; `count` is u32 so `count*512` fits usize on 64-bit"), or use `checked_mul`/`saturating` to make it locally obvious and robust to a future `len_bytes` that lies. The range check already maps to `STATUS_BAD_REQUEST`; folding the multiply into a `checked_*` that also returns `STATUS_BAD_REQUEST` costs nothing and removes the reasoning burden.

### S-2. `restore` length-check multiply is unchecked (32-bit theoretical, harden anyway)

blk.rs:275: `if bytes.len() != SECTION_FIXED + n * SECTION_PER_CLUSTER`. `n` is `u32`-as-`usize`; `SECTION_PER_CLUSTER == 65576`. On 64-bit, max product `~2.8e14` cannot overflow `usize` (verified) and `HashMap::with_capacity(n)` runs only *after* the length equality check, so a hostile `n` cannot cause a pre-allocation DoS (it would need `bytes.len()` to actually be ~2.8e14 to pass). **The ordering is correct and the 64-bit math is safe.**

On 32-bit `usize` the multiply would wrap and a crafted `(n, bytes.len())` pair could pass the check then drive `with_capacity(n)` / the indexing `at = SECTION_FIXED + i * SECTION_PER_CLUSTER` into OOB or huge alloc. Not a real target here.

**Suggestion:** use `n.checked_mul(SECTION_PER_CLUSTER).and_then(|x| x.checked_add(SECTION_FIXED))` and treat `None` as `RestoreError`. Belt-and-suspenders; documents intent; immune to any future 32-bit build or `SECTION_PER_CLUSTER` growth.

### S-3. Silent ignore of an 8-byte write at REG_COUNT (0x18) is deterministic but a guest-debugging trap

The bus pre-validates 4/8-byte natural alignment, so an 8-byte access at `0x18` (8-aligned) reaches the device and spans COUNT(0x18)+CMD(0x1C). `mmio_write`'s `match (off, len)` has no `(0x18, 8)` arm → falls to wildcard `_ => {}` (blk.rs:245). So an 8-byte COUNT+CMD write is **silently dropped**: COUNT not latched, CMD never fires, STATUS unchanged.

This is deterministic and acceptable per the trait contract ("unknown offsets are ignored"). But it is a nasty guest-driver trap: a driver that "helpfully" writes COUNT and CMD as one 8-byte store sees *nothing happen* and STATUS stuck at its prior value — no fault, no signal. Similarly a 4-byte read at `0x08`/`0x0C` (halves of the 8-byte SECTOR) reads zeros rather than the low/high half.

**Suggestion:** no code change required, but document in §6.5 (and ideally a `mmio_write` comment) that the registers accept *only* their natural width at their natural offset; sub-register and span accesses are no-ops/zeros by design. Optionally, a debug-build `log_dev_event` on a write to a known register at the wrong width would turn a silent guest bug into an observable one without affecting release determinism. (Be careful: any such log must be a canonical record or it must not depend on the path — prefer documentation over logging here to avoid a new input.)

### S-4. `FileBase::read_at` collapses all `io::Error`s to `BaseIoError` — including EOF races

blkfile.rs:693-705 caches `len` at `open` and, for in-range reads, calls `read_exact_at(&mut buf[..take], offset)`. `read_exact_at` loops internally over short reads and **retries `ErrorKind::Interrupted` (EINTR)** per the std contract, so EINTR is handled correctly — good. But if the file is *truncated* between `open` and the read, `read_exact_at` hits EOF mid-buffer → `UnexpectedEof` → mapped to `BaseIoError` → `STATUS_HOST_IO`. That is the right classification (a shrinking "immutable" base is a host fault), and it is deterministic-on-replay only in the sense that replay would re-hit the same host fault and re-fault the slot — consistent with the `STATUS_HOST_IO` "does not replay, slot-fatal" contract.

**Suggestion:** add a comment at the `map_err(|_| BaseIoError)` noting the two distinct failure modes folded together (genuine read error vs. base-shrank-under-us), both legitimately host faults. No behavior change. This makes the "immutable base" contract's failure semantics explicit at the seam.

### S-5. `FileBase::len`/`is_empty` are public API on a type with no other accessors — confirm they are used

blkfile.rs:678-685 exposes `len()` and `is_empty()`. `len()` is exercised by a test (`reads_serve_file_content_and_zero_fill_past_eof` asserts `base.len()`), and clippy's `len_without_is_empty` lint is presumably why `is_empty` exists. That is fine. Just flag for the author: if nothing outside tests consumes `len()`, consider `#[cfg(test)]` or documenting it as an observability accessor, so the public surface of the production backend stays minimal.

### S-6. Missing test cases (the device is correct, but coverage has gaps)

The test suite is strong, but three edges are untested:

1. **restore → snapshot byte-identity.** `snapshot_is_sorted_deterministic_and_roundtrips` checks snapshot determinism and restore round-trip *behavior*, but never asserts that `restore(snapshot(d))` followed by `snapshot` reproduces the original bytes. Since `restore` rebuilds the HashMap and `snapshot` re-sorts, this should hold — worth a direct assertion to lock the codec.
2. **Write at the exact capacity boundary.** Tests cover `sector == capacity` and `sector == u64::MAX` (both BAD_REQUEST) and cross-cluster writes, but not a write whose `end_sector == capacity` exactly (the last valid sector) — the `end_sector > capacity` vs `>=` boundary. (Code is correct: `>` allows `end_sector == capacity`. A test would pin it.)
3. **Multi-cluster fully-overlaid read.** `cross_cluster_requests_split_correctly` reads a span that mixes base and overlay. A read of ≥2 clusters that are *entirely* overlaid (no base fallthrough) would exercise the `overlay.get` branch across a cluster boundary with no base read at all.

None of these are bugs; they are coverage hardening for a device whose correctness other beads will depend on.
