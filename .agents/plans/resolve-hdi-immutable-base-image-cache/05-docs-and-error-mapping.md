# Docs And Error Mapping

## Documentation Updates

Add a short decision document:

```text
docs/decisions/base-image-cache-contract.md
```

Use the existing decision-doc style in `docs/decisions/`. The document should
record:

- Worker image-cache entries are content-addressed by lowercase BLAKE3 hex.
- Boot blobs are copied into memory with their existing caps.
- Base images are also copied into owned immutable process memory before pv-blk
  sees them.
- The accepted base-image cap is `MAX_BASE_IMAGE_BYTES = 512 MiB`.
- A cache entry larger than the cap fails CreateVm/Restore/Fork/VerifyReplay
  before hashing.
- A cache entry that mutates after resolution cannot affect an already-created
  runtime because pv-blk reads from owned bytes.
- Future large-image support must introduce a new immutable streaming backend
  rather than relaxing this cap silently.

Update existing docs only if they currently imply the old unsafe invariant:

- `crates/dh-vmm/src/blkfile.rs` module comments are code comments, not docs,
  but must be corrected.
- `docs/ops/test-partitioning.md` currently says `DH_M9_IMAGE_CACHE` entries
  are the bytes the worker resolves; it can remain true, but add one sentence
  if needed that worker runtimes detach base-image bytes from the cache after
  verification.
- Avoid broad edits to `.agents/docs/`; those are upstream-synced planning
  docs and should not be churned for this local implementation unless a
  specific divergence must be recorded.

## Error Mapping

Keep `ImageResolverError` variants meaningful:

- `TooLarge { kind: BaseImage, len, max }` for oversized base images.
- `HashMismatch { kind: BaseImage, expected, actual }` for wrong bytes under a
  content-addressed filename.
- `NotFile { kind: BaseImage }` for directories, symlinks, and other non-regular
  entries.
- `Io { kind: BaseImage }` for actual host read failures.
- `AllocationFailed { kind: BaseImage, requested, .. }` or equivalent for
  fallible owned-buffer allocation failure.

`service.rs` should continue mapping:

- `TooLarge` -> `Status::invalid_argument`
- `HashMismatch` -> `Status::data_loss`
- `NotFound` / `NotFile` -> `Status::failed_precondition`
- `Io` -> `Status::unavailable`
- `AllocationFailed` -> `Status::resource_exhausted`

Do not collapse these into one generic CreateVm error. The bead acceptance
explicitly requires actionable error mapping.

## Comments To Remove Or Reword

Reword any comment equivalent to:

> The production FileBase opens O_RDONLY, therefore the base image is immutable.

Correct replacement:

> O_RDONLY prevents writes through this fd, but does not freeze the inode.
> Worker cache resolution uses owned verified bytes for untrusted cache entries;
> direct file-backed `FileBase` remains for trusted fixture/direct paths.
