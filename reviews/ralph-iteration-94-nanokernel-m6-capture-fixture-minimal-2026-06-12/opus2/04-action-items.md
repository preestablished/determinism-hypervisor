# Action Items

## Action Items

### Critical
- [ ] None.

### Important
- [ ] None.

### Suggestions
- [ ] [asm/capture_fixture.asm module header / src/lib.rs `capture_fixture_elf()` doc] (S1) Add a one-line note that the framebuffer is raw known-pattern content with **no `FbInfo` descriptor at offset 0** — this fixture targets the C2 by-name/layout_version and C5 neutrality paths, not the C4 `FbInfo`-decode path (ARCH §6.8). Prevents a future C4 capture path from being wired against it and parsing garbage dimensions.
- [ ] [asm/capture_fixture.asm:48-58] (S2) Either add the `BOOTINFO_OFF_MAGIC == BOOTINFO_MAGIC` guard before the cmdline parse (true parity with `landing_loop.asm:36-44`) or soften the "same parse contract as landing_loop" comment, since the loops are not byte-identical without it.
- [ ] [asm/capture_fixture.asm:48-58] (S3) Optional half-line comment that `layout_version` is an author-supplied/trusted knob (64-bit accumulate narrowed to u32 can wrap on a pathological cmdline) so nobody later mistakes it for a hardened parser.
- [ ] [tests/capture_manifest_interop.rs:29-41] (S4) Add a one-line comment explaining the channel header is built by hand on purpose (to prove the asm's literal bytes pass the real attach) so a future DRY refactor doesn't collapse it into `ChannelHeader::canonical()` and lose the byte-level guarantee.
- [ ] [tests/capture_manifest_interop.rs:139-145] (S5) Optionally add the exact-fit boundary case (read the final 8 bytes at `FB_BYTES - 8` into an 8-byte buffer succeeds) alongside the existing over-read-refused case to pin the off-by-one in both directions.
