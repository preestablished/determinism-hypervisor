# Suggestions

## Make the boot observer explicitly diagnostic/test-only API

- File: `crates/dh-worker/src/service.rs:156`

The new observer is public because integration tests need access to it, but it now appears as part of `dh_worker::service`'s public API. That is acceptable for this branch, but it would be clearer to mark it as diagnostic-only and avoid implying a stable production contract. The functions also count boot attempts, not successful loads.

```rust
#[cfg(target_arch = "x86_64")]
#[doc(hidden)]
pub mod boot_observer {
    // Diagnostic counters used by integration/acceptance tests.
}
```

## Make cache population race-tolerant for parallel acceptance runs

- File: `crates/dh-worker/tests/common/mod.rs:205`

`ensure_cache_entry` checks `dest.exists()` and then tries `hard_link`, falling back to `copy` for any error. If two ignored Linux acceptance tests are run in parallel against the same cache, one process can create the destination after the existence check, causing the other to copy over an already-valid entry. This is test harness code, but the fix is cheap and avoids noisy artifact-cache races.

```rust
fn validate_existing(dest: &Path, hash: [u8; 32]) -> TestResult<[u8; 32]> {
    if hash_file(dest)? == hash {
        Ok(hash)
    } else {
        Err(format!("image cache entry {} hash mismatch", dest.display()))
    }
}

match std::fs::hard_link(source, &dest) {
    Ok(()) => Ok(hash),
    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => validate_existing(&dest, hash),
    Err(_) => match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&dest)
    {
        Ok(mut out) => {
            let mut input = std::fs::File::open(source)
                .map_err(|e| format!("open {}: {e}", source.display()))?;
            std::io::copy(&mut input, &mut out)
                .map_err(|e| format!("copy {} to {}: {e}", source.display(), dest.display()))?;
            validate_existing(&dest, hash)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            validate_existing(&dest, hash)
        }
        Err(e) => Err(format!("create image cache entry {}: {e}", dest.display())),
    },
}
```

At minimum, handle `AlreadyExists` before falling back to overwrite-capable copy:

```rust
match std::fs::hard_link(source, &dest) {
    Ok(()) => Ok(hash),
    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
        if hash_file(&dest)? == hash {
            Ok(hash)
        } else {
            Err(format!("image cache entry {} hash mismatch", dest.display()))
        }
    }
    Err(_) => { /* copy into a newly-created destination */ }
}
```

## Put the exact ignored-test commands next to the tests

- File: `crates/dh-worker/tests/replay_engine.rs:869`
- File: `crates/dh-worker/tests/restore_engine.rs:878`

The commands exist in prompt material, but the tests themselves only say they require artifacts and KVM. Adding the exact command to the ignore message or nearby comments would make it harder for operators to run the wrong target or accidentally use skip mode as acceptance evidence.

```rust
#[ignore = "M9 Linux gate: DH_M9_ALLOW_SKIP=0 cargo test -p dh-worker --test replay_engine linux_boot_once --release -- --ignored --nocapture"]
```
