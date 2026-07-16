# Critical & Important

## Critical

**None.** The framing codec is total and correct over untrusted bytes. Every
overflow path was probed (forged 4 GiB−1 len, missing pad bytes, exact-EOF fit,
zero-length sections, forged `count = u32::MAX`) and yields `Ok`/`Err`, never a
panic and never an allocation DoS. The `offset += padded_end` increment is safe:
`padded_end <= bytes.len() - offset` is checked first (line 226), so `offset`
can never exceed `bytes.len()`, and on 64-bit `usize` the small `offset` plus a
bounded `padded_end` cannot overflow. `cargo test -p dh-snapshot` is green.

---

## Important

### I-1. ENTR tag collides two incompatible payloads — entropy DEVICE regs (16B) vs PRNG state (56B). Flag for bead 6yl.

**Files:**
- `crates/dh-snapshot/src/dhsnap.rs:76` — `0x0004 => Some(tag::ENTR)`
- `crates/dh-snapshot/src/dhsnap.rs:310-347` — `EntrSection` (56-byte PRNG state)
- `crates/dh-devices/src/entropy.rs:44-46,168-172` — device `snapshot()` emits 16 bytes
- `.agents/docs/determinism-hypervisor/API.md:618` — §4 `ENTR` contents = PRNG state, 56 bytes

**This is not a defect in 68l's code** — the codec is internally consistent and
deliberately scopes itself to framing. But the diff hard-wires a contradiction
that the integration bead (6yl) will hit, and the codec's own doc comments paper
over it, so it must be flagged loudly before 6yl wires anything.

The contradiction:

1. `tag_for_device_id(0x0004) -> ENTR`. So if the snapshot engine frames devices
   generically by calling `DetDevice::snapshot(0x0004)` and tagging the result by
   `tag_for_device_id`, it writes an `ENTR` section whose contents are the device
   registers: `buf_gpa u64 || len u32 || status u32` = **16 bytes**
   (`crates/dh-devices/src/entropy.rs:168-172`, `SECTION_LEN = 16`).

2. But §4 (API.md:618) and `EntrSection` (dhsnap.rs:310, `LEN = 56`) define `ENTR`
   contents as the ChaCha20 PRNG state `seed[32] || stream u64 || word_pos u128`
   = **56 bytes**. That state is VMM-owned, not device-owned
   (`entropy.rs:45`: "The PRNG state is NOT here — it is the VMM-owned ENTR
   section (§5)").

So a naive generic engine would emit a 16-byte `ENTR` that violates §4, and
`EntrSection::decode` would reject it on restore (`BadLength { found: 16 }`,
dhsnap.rs:338). Conversely, if the engine emits the 56-byte VMM PRNG state as
`ENTR`, there is **nowhere in the §4 table for the entropy device's
`{buf_gpa, len, status}` registers to go** — I grepped the entire spec; no tag
covers them. They are simply unrepresented.

**Position / fix for 6yl (concrete):**

The cleanest resolution is to define `ENTR` as the union the spec already half-
implies and that `EntropyState`/`EntrSection` already mirror: the device regs are
operationally reconstructible (`buf_gpa`/`len` are guest-written MMIO that the
guest re-establishes; `status` is transient), so the *only* state that must
survive a fork for bit-identical replay is the 56-byte PRNG tuple — which is
exactly what `EntrSection` encodes and what §5 calls the "VMM-owned ENTR
section." That argues the device's 16-byte `snapshot()` blob should **not** be
framed as `ENTR` at all; the snapshot engine must special-case device 0x0004:
take the PRNG state from the VMM (the `EntropyState`/`DetEntropy::state()` path,
`entropy.rs:71-77`) as the `ENTR` contents and **drop** the device's register
blob (or fold `buf_gpa`/`len`/`status` into a `sec_version`-bumped ENTR layout if
any of them is truly needed across restore — they appear not to be).

Either way bead 6yl must:
- decide whether the 16-byte device regs survive a snapshot at all, and if so where;
- if they merge into `ENTR`, bump `EntrSection::VERSION` and the §4 layout together;
- make `tag_for_device_id(0x0004)` either stop being used for the generic device
  framing path, or be accompanied by an explicit "ENTR contents come from the VMM,
  not `DetDevice::snapshot`" carve-out in the engine.

**Why Important not Critical:** nothing in *this* diff is wrong or untested; the
landmine only detonates when 6yl wires the generic engine. But because the codec
ships the device-id map and the `EntrSection` struct as the authoritative §4
artifacts, this is the right diff to attach the warning to, and the resolution
constrains how `EntrSection` may evolve. Recommend a one-line doc note at
dhsnap.rs:76 ("`ENTR` contents are VMM-owned PRNG state, NOT this device's
`snapshot()` regs — see bead 6yl") so the next reader of the map isn't misled.
