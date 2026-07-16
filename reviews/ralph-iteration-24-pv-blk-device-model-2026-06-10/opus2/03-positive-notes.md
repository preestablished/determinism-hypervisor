# Positive notes — pv-blk

### P-1. The HashMap-order leak is closed *and* tested with an adversarial case
The one place iteration order could reach guest-visible bytes is `snapshot`. It collects keys, `sort_unstable()`s, then serializes — and `snapshot_is_sorted_deterministic_and_roundtrips` builds two devices that dirty the same clusters in **opposite** order and asserts byte-identical overlay output (blk.rs:529-560). This is exactly the right adversarial test for a deterministic hypervisor; it would catch a regression to naive `for (k,v) in &overlay`.

### P-2. The `BlockBase` seam keeps host I/O out of the deny-listed crate, by construction
`dh-devices` forbids `std::fs`/`std::time`/host randomness (enforced by clippy lints *and* the `no_host_ambient_authority` source-grep test). The device touches the file only through the `BlockBase` trait; the `O_RDONLY` `FileBase` impl lives in `dh-vmm`. The new `blk.rs` adds no deny-listed token, so the grep gate stays green. This is the correct architectural split and the rationale is documented inline.

### P-3. CoW immutability is enforced by the OS, not just by discipline
`FileBase::open` uses `File::open` (read-only fd). The fd physically cannot write, so "base bytes and mtime never change" holds by construction, not by hoping the code never writes. The test crate underscores this with `VecBase(Rc<Vec<u8>>)` — an `Rc` shared base proves at the type level the device cannot mutate it.

### P-4. The §6.5 acceptance test actually fails if the device cheats
`base_file_bytes_and_mtime_unchanged_after_writes` does a real guest write through the device, then asserts the on-disk bytes equal the original content *and* mtime is unchanged. The byte-equality assertion is the load-bearing one (it would fail if any write leaked to the base), so the test is not a tautology — even on a fast CI box where mtime-before == mtime-after happens to hold trivially, the bytes-differ check still catches a base mutation. The mtime check is a useful belt-and-suspenders on top.

### P-5. Cluster key derivation is consistent everywhere — byte-derived, never sector-derived
`do_read`, `do_write`, and `snapshot` all key the overlay on `off / CLUSTER_SIZE` where `off` is the byte offset (`sector * 512`). There is no place that derives a cluster from a sector directly, so the read path, write path, and snapshot agree on cluster identity. I specifically looked for a sector-vs-byte mismatch (a classic CoW bug) and found none.

### P-6. Restore is hardened against malformed input in the right order
`restore` checks `sec_version` and a minimum length first, reads the header, then checks `bytes.len() == SECTION_FIXED + n*SECTION_PER_CLUSTER` **before** `HashMap::with_capacity(n)` — so a hostile `n` cannot trigger a pre-allocation DoS (the exact-length check gates it). Each cluster's blake3 digest is verified before insertion, catching bit-rot/truncation. The tampered-byte, wrong-version, and truncated-input refusals are all tested.

### P-7. Partial-failure determinism is reasoned about, not accidental
`do_write`'s comment (blk.rs:202-204) explicitly notes that a guest-fault after the RMW leaves the cluster populated, and frames it as "a deterministic function of the same failing request." The author clearly thought about partial-failure replay semantics rather than stumbling into them. (My I-1 asks only that `do_read`'s symmetric partial-write side effect get the same explicit treatment.)
