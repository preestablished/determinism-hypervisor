# Action Items

## Critical

- None.

## Important

- None.

## Acceptance Follow-Up

- [ ] Run the Linux artifact gate on a host with `DH_M9_BZIMAGE`, `DH_M9_INITRAMFS`, `DH_M9_BASE_IMAGE`, `DH_M9_GAME_IMAGE`, and `DH_M9_IMAGE_CACHE` set:

```bash
DH_M9_ALLOW_SKIP=0 cargo run -p dh-cli -- gate --linux --runs 2 --bzimage "$DH_M9_BZIMAGE" --initramfs "$DH_M9_INITRAMFS" --base-image "$DH_M9_BASE_IMAGE" --game-image "$DH_M9_GAME_IMAGE"
```

## Suggestions

- [ ] `tools/dh-cli/src/linux.rs:268` Consider including `ready_payload_len` in the Linux gate fingerprint for better diagnostics and closer alignment with later Ready payload stability gates.

