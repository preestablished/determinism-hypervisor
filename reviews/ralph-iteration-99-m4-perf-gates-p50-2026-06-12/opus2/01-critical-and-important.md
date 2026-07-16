# Critical & Important Findings

## CRITICAL

### C1 — `harness = false` bench with a fully-cfg'd-out crate root breaks the aarch64 CI lane

- **File:** `crates/dh-worker/benches/perf_gates.rs:11` (`#![cfg(target_arch = "x86_64")]`) + `crates/dh-worker/Cargo.toml` (the new `[[bench]] name = "perf_gates" harness = false`)
- **Severity:** Critical (hard CI failure on a lane that exists and gates merges)

**Problem.** The bench crate root carries `#![cfg(target_arch = "x86_64")]`, which gates out the *entire* file body — including `fn main()` (lines 177-181) — on any non-x86 target. Because the target is declared `harness = false`, there is no libtest harness to synthesize a `main`. On a non-x86 build the crate is therefore empty with no entry point.

The repo's CI (`.github/workflows/ci.yaml:39-40, 77-81`) has an `ubuntu-24.04-arm` (aarch64) host lane that runs:

```
cargo clippy --workspace --all-targets -- -D warnings
cargo build  --workspace
cargo test   --workspace
```

`--all-targets` and the default `cargo build --workspace` both compile bench targets. On aarch64 the bench compiles to an empty crate → `error[E0601]: main function not found in crate perf_gates`. The arm lane fails.

Note why the sibling `tests/*.rs` files (e.g. `tests/snapshot_engine.rs`) get away with the identical `#![cfg(target_arch = "x86_64")]` crate-root gate: the **default test harness** emits the `main` for them, so an empty test crate still links. A `harness = false` target has no such backstop.

**Reproduced.** A minimal crate with `#![cfg(target_arch = "x86_64")]` + `fn main(){}` + `[[bench]] harness = false`, built `--target aarch64-unknown-linux-gnu`:

```
error[E0601]: `main` function not found in crate `b`
  consider adding a `main` function to `benches/b.rs`
```

**Fix.** Keep the x86-only body cfg'd, but always provide a `main`. Two clean options:

Option A — move the gate off the crate root and add a non-x86 stub main:

```rust
// top of benches/perf_gates.rs — NO crate-root cfg
#![allow(clippy::type_complexity)]

#[cfg(target_arch = "x86_64")]
mod x86 {
    // ... all current imports, helpers, benches(), and the real driver ...
    pub fn run() {
        let mut c = criterion::Criterion::default().configure_from_args();
        benches(&mut c);
        c.final_summary();
    }
}

#[cfg(target_arch = "x86_64")]
fn main() { x86::run(); }

#[cfg(not(target_arch = "x86_64"))]
fn main() {} // arm lane: empty trend instrument, nothing to bench
```

Option B (smaller diff) — keep `#![cfg(target_arch = "x86_64")]` on everything else but pull `main` outside the gate by splitting into a cfg'd `main` pair. The crate-root attribute must be removed either way, because a crate-root `#![cfg(...)]` that is false eliminates *all* items including any `#[cfg(not(...))] fn main`.

**Verify after fix:** `cargo build --benches --target aarch64-unknown-linux-gnu -p dh-worker` (or `cargo clippy --all-targets`) must link on arm, and `cargo build --benches` must still build on x86.

---

## IMPORTANT

### I1 — Incremental-snapshot p50 measures the server-DEDUPED path, not a cold 8k-page write

- **File:** `crates/dh-worker/tests/perf_gates.rs:146-177` (and the bench's mirror, `benches/perf_gates.rs:113-145`)
- **Severity:** Important (measurement validity; methodology-note correctness)

**Problem.** Every one of the 30 snapshot samples ships byte-identical page content. The fill loop writes a deterministic per-page byte that never varies across iterations:

```rust
for page in 0..DIRTY_PAGES {
    slot.guest_mem.write_slice(&[page as u8 ^ 0x5A], GuestAddress(page * 4096)).unwrap();
}
```

That fill runs **once** before the sample loop; the loop body only rebuilds the *dirty-set bitset* (`dirty.insert(page)`), it does not rewrite guest RAM. So each sample re-reads and re-uploads the same 8192 page payloads.

The store dedups globally by content hash. Trace:
- `snapshot_engine.rs:161` → `put_snapshot_from_parts` → `put_pages` (`client.rs:94`), which BLAKE3-hashes each page (`client.rs:137`).
- pagestore ingest (`snapstore-pagestore/src/ingest.rs:265-267`): `if let Some(loc) = self.index.get(&hash) { newly_written: false }` — a global dedup hit, no pack write. Confirmed by the in-tree test `dedup_across_batches` ("Second batch: all dedup hits").

Consequence:
- The **root** full snapshot + **sample 1** write these 8192 pages for the first time → real pack writes + fsync.
- **Samples 2..30** are all dedup hits → zero page-data bytes written; only the DHSNAP container/manifest is assembled and put.
- The reported `p50` is `samples[15]` after sort — squarely inside the deduped regime.

So the 111.6 ms p50 is **manifest-put + container fsync over an empty page-write set**, not the cold "ship 8k pages → 32 MiB → fsync" the gate is meant to measure. (Conclusion direction: the true cold first-shot cost is *higher*, or — if the manifest/container fsync dominates regardless of page writes — the page write is not where the time goes. Either way the number is not what the methodology comment claims.)

The guard `assert_eq!(out.pages_shipped, DIRTY_PAGES)` does **not** catch this: `pages_shipped = pages.len()` is computed pre-upload (`snapshot_engine.rs:149`), before dedup, so it is always 8192 regardless of how many pages actually hit disk.

**Why it still doesn't flip the verdict:** snapshot already FAILs (111.6 ms ≫ 15 ms) even on the cheaper deduped path, so the human-decision escalation (8ot) stands. But the test's own header claims it times "the engine path the gate times" / "32 MiB of I/O", and for 29 of 30 samples that I/O does not happen.

**Fix (choose one):**
1. **Defeat dedup — make samples write VARYING content** so each sample exercises a real cold write. Mix an iteration counter into the fill, re-running it *inside* the timed-setup region (outside `Instant::now()`):
   ```rust
   for s in 0..SAMPLES {
       for page in 0..DIRTY_PAGES {
           slot.guest_mem
               .write_slice(&[(page as u8 ^ 0x5A).wrapping_add(s as u8)], GuestAddress(page * 4096))
               .unwrap();
           dirty.insert(page).unwrap();
       }
       let t = Instant::now();
       /* take_snapshot ... */
   }
   ```
   This makes the p50 reflect the genuine cold 8k-page write the gate intends. (Caveat: this grows the tempdir store by ~32 MiB/sample = ~960 MiB; acceptable for a deliberate quiesced-box run, but note it in the header.)
2. **Or**, if measuring the *deduped warm-resnapshot* path is the deliberate intent, say so explicitly in the module header and rename the gate's prose — but that contradicts "incremental snapshot at 8k dirty pages" as a cold-cost gate, so option 1 is almost certainly correct.

At minimum, add a line to the methodology comment acknowledging content-identity across samples and which path is being measured. This is the single most consequential correctness-of-measurement issue in the change.
