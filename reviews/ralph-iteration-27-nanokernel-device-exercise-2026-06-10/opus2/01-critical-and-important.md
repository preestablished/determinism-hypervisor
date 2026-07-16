# Critical and Important Findings

## CRITICAL

### C1 — Ring-W descriptor size `0x1E0000` is not a power of two; the host rejects it at attach, so the guest never reaches stage `D` or `X`

**File:** `tests/nanokernel/asm/device_exercise.asm:160`
(disassembled: `1001cb: movl $0x1e0000,0x2c(%rbx)`)

The guest writes the channel header's ring-W descriptor as
`{offset 0x20000, size 0x1E0000}`:

```asm
mov     dword [rbx + 0x28], 0x20000  ; W offset
mov     dword [rbx + 0x2C], 0x1E0000 ; W size
```

`0x1E0000 = 1,966,080` is **not a power of two**. The host side this guest
exists to interop with — `Channel::attach`
(`guest-sdk/crates/detguest-host/src/channel.rs:152`) calling
`RingDesc::validate` (`guest-sdk/crates/detguest-wire/src/header.rs:204`) —
requires every ring size to be a power of two:

```rust
// channel.rs attach()
if d.size == 0 || !d.size.is_power_of_two() {
    return Err(AttachError::BadRingSize { ring });
}
// header.rs RingDesc::validate()
if self.size == 0 || !self.size.is_power_of_two() {
    return Err(DecodeError::BadField);
}
```

So `attach` returns `Err(AttachError::BadRingSize { ring: RingId::W })`.
`AttachError::init_status()` (channel.rs:59) maps every non-Mem/non-Already
variant to `InitStatus::BadMagicVersion` = **status 2**. The detchannel PIO
handler (`crates/dh-devices/src/detchannel.rs:298 channel_init`) stores that
status; the guest reads it back via `IN 0xD37C`:

```asm
mov     eax, CHANNEL_PAGES        ; 512 (correct)
mov     dx, PORT_INIT_GO
out     dx, eax
in      eax, dx                   ; status: 2, NOT 0
test    eax, eax
jnz     .fail_d                   ; <-- taken
```

`.fail_d` emits lowercase `'d'` and returns into crt0's HLT park. **The
Beacon write, producer-index publish, and doorbell are all dead code.** The
serial log the harness reads is **`CEPBd`**, never the required `CEPBDX`
(`DEVICE_EXERCISE_OK_SEQUENCE = b"CEPBDX"` in `src/lib.rs`). The M1
acceptance the bead is named for cannot pass.

**Proof (executed, not reasoned):** I added a scratch integration test under
`guest-sdk/crates/detguest-host/tests/` that lays out the guest's exact
header bytes and runs the real `Channel::attach`:

```
attach(W=0x1E0000) -> Some(BadRingSize { ring: W })
```

and, with the SDK-canonical size, attach + drain works perfectly:

```
attach(W=0x10_0000) + put Beacon(len 24, kind 5, seq 0, vnanos arbitrary) →
  drained 1 event: Beacon { beacon_id: 45887 = 0xB33F }, ring W, seq 0
```

This second result is important: it confirms **every other byte the guest
writes is correct** — record framing, `len 24`, `kind 5`, `seq 0`, arbitrary
`vnanos`, the 8-byte payload, and `prod = 24`. The drain decodes one clean
Beacon. The W ring **size** is the only defect.

**Root cause:** `.agents/docs/guest-sdk/ARCHITECTURE.md`'s channel-layout
table is self-contradictory — it says "Indices are free-running `u32`, masked
by `size - 1` (sizes are powers of two)" and then lists
`ring W data (1,966,080 bytes = 0x1E0000)`. The author copied the literal
table value and missed the invariant two lines above it. The guest-sdk
already resolved this in code: `header.rs` lines 92–103 document the decision
to size W at the largest power of two that fits (`RING_W_SIZE = 0x10_0000`,
1 MiB) with `0x12_0000..0x20_0000` reserved, because "free-running u32
indices break at u32 wraparound for any size that does not divide 2^32" and
both validation paths reject non-power-of-two descriptors.

**Fix (one line):**

```asm
mov     dword [rbx + 0x2C], 0x100000 ; W size = 1 MiB (SDK canonical, power of two)
```

Also update the module-header comment, which currently documents `ring data
C/I/A/W at 0x8000/0xC000/0x10000/0x20000` (fine) but does not state the W
size; add "W size 0x100000 (1 MiB; the SDK's canonical RING_W_SIZE — the
ARCHITECTURE.md table's 0x1E0000 is a non-power-of-two doc bug)". The Beacon
still goes at W offset 0, so no other change is needed; offset 0 < 1 MiB and
the record (24 bytes) fits with no wrap. My second scratch test confirms this
exact layout drains to the expected `Beacon{0xB33F}`.

> Note on the index masking the prompt flagged: with the buggy `0x1E0000`
> size, `mask = size - 1 = 0x1DFFFF` is **not** a contiguous low-bit mask, so
> `pos & mask` would alias positions and corrupt the ring — which is *exactly*
> the failure mode the power-of-two rule exists to prevent. The guest never
> gets there (attach rejects first), but it confirms `0x1E0000` is unusable,
> not merely "unaccepted by a strict validator."

---

## IMPORTANT

### I1 — No test asserts the guest's channel header actually attaches; the C1 bug shipped with all tests green

**Files:** `tests/nanokernel/tests/elf_shape.rs`, `tests/nanokernel/src/lib.rs`

`cargo test -p nanokernel` passes (I ran it: 6/6 green). The only checks on
`device_exercise` are: the ELF is embedded and non-empty, and it is a static
x86-64 exec at the load address. Nothing executes the guest, and — more to
the point for an *interop* program — nothing asserts that the 0x40-byte
channel header the guest writes is one the host's `Channel::attach` will
accept. The whole purpose of this guest is the `D`/`X` detchannel stages, yet
the one thing that breaks them is invisible to CI.

This is the highest-value follow-up. Because `detguest-host` /
`detguest-wire` are sibling path-deps already linked by this workspace
(`crates/dh-devices` depends on `detguest-host`), an integration test in this
repo can encode the guest's canonical header byte-for-byte and assert
`Channel::attach(...).is_ok()` plus `drain_events` yields exactly one
`Beacon { beacon_id: DEVICE_EXERCISE_BEACON_ID }`. That test would have caught
C1 and will catch any future drift between the guest's hand-written header and
the SDK's `ChannelHeader::canonical()`. Even better: assert the guest's header
bytes equal what `ChannelHeader::canonical().write_to(&mut [0u8; 0x40])`
produces — that pins offset, size, magic, and version in one shot.

(Counterpart for the device stages: an emulator-driven run that feeds the ELF
to the VMM and checks the serial output equals `CEPBDX` is the real
end-to-end gate, but the header-attach unit test is the cheap, deterministic
catch for C1 specifically.)

### I2 — The ARCHITECTURE.md layout-table contradiction that caused C1 is not tracked anywhere in this change

**File:** `.agents/docs/guest-sdk/ARCHITECTURE.md` (channel-layout table,
the `ring W data (1,966,080 bytes = 0x1E0000)` row)

The doc the guest was clean-roomed from actively misleads: its layout table's
W-size value violates the power-of-two invariant the same table asserts, and
the value `0x1E0000` is what the guest copied. The guest-sdk resolved this in
its own source (`header.rs:92-103`, "Tracked as a spec documentation issue")
but **this repo's** copy of the doc still carries the contradiction, and this
iteration's commit neither fixes the doc nor files a bead against it. The next
person clean-rooming any channel-touching guest from this doc will reproduce
C1. File a doc-bug bead and either correct the table to `0x100000 (1 MiB,
0x120000..0x200000 reserved)` or annotate the row with the resolution. This is
"Important" rather than "Critical" only because it is documentation, not
shipped behaviour — but it is the actual root cause and will recur if left
alone.
