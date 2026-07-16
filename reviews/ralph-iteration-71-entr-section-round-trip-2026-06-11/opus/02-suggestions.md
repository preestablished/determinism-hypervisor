# Suggestions (non-blocking)

### S1 — No sec_version dispatch helper; each caller will hand-roll the v1/v2 branch

**Location:** `crates/dh-snapshot/src/dhsnap.rs` (ENTR decode), future engine callers

The container layer does not enforce per-tag versions — it stores `sec_version` and
hands the raw `contents` to the caller (confirmed: `Container::get` returns
`{sec_version, contents}` and the typed `decode(bytes, sec_version)` does the version
check). So any reader that wants "the ENTR PRNG state, whatever the version" must itself
write:

```rust
let prng = match sec.sec_version {
    1 => EntrSection::decode(sec.contents, 1)?,
    2 => EntrSectionV2::decode(sec.contents, 2)?.prng(),
    v => return Err(...),
};
```

Today there are **zero** such callers (`grep` finds no engine code consuming ENTR yet —
the only consumers are the two new tests). So this is genuinely minor *now*. But the
moment the restore path is wired, this match will appear, and if more than one site needs
it (restore + verify + fork), it will be copy-pasted. Consider a single helper, e.g.
`EntrSection::decode_any(contents, sec_version) -> Result<EntrSection, _>` (returning the
PRNG state, the common denominator), or a small `enum EntrAny { V1(EntrSection),
V2(EntrSectionV2) }` with a `prng()` accessor. Land it when the first real consumer
arrives, not speculatively.

### S2 — `from_parts` re-parses bytes the device already had structured

`EntrSectionV2::from_parts(prng, device_regs: &[u8])` takes the device's 16-byte blob and
re-parses it (`u64::from_le_bytes(...)` etc., `dhsnap.rs:99-105`). Then `device_regs()`
re-serializes it on the way out. The seam is correct and the round-trip test proves
losslessness, but it does mean the device-reg layout is now encoded in *two* places (here
and in `entropy.rs::snapshot`/`restore`). That duplication is the load-bearing risk I1/I2
are about. If `dh-snapshot` is allowed to depend on `dh-devices` types (it now has the
dev-dep, but this would be a real dep), `from_parts` could take the structured
`(buf_gpa, len, status)` tuple or a shared reg struct instead of a `&[u8]`, eliminating
the parse/reserialize and the second copy of the offsets. Weigh against the layering cost
of a real (non-dev) dependency.

### S3 — `stream` is never exercised with a non-zero engine-produced value

The golden test uses a real burned state, but `DetEntropy::from_seed` never calls
`set_stream`, so `stream` is always 0 in practice (confirmed in `entropy.rs:64-85` —
`stream` only becomes non-zero via `restore`). The second test
(`v1_and_v2_sections_coexist_and_misuse_is_loud`) does set `stream: 5, word_pos: 99`
through `from_parts`/`encode`/`decode`, so the *field* is byte-pinned with a non-zero
value — good. This is fine as-is; noting only that the "live" golden path can never
observe a non-zero stream because nothing in the current code sets one. If a future
multi-stream design lands, add a live restore-with-stream case.

### S4 — `Default` derive on `EntrSectionV2` is unused and slightly surprising

`#[derive(..., Default)]` (`dhsnap.rs:368`) gives an all-zero v2 section, which is not a
meaningful entropy state (zero seed, zero word_pos). v1 `EntrSection` also derives
`Default`, so this is consistent with the existing pattern — keep it for symmetry, but it
is dead surface today. No action needed unless you are trimming public API.
