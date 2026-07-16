# Critical and Important Findings

## CRITICAL

### C1 — Ring W descriptor size `0x1E0000` is not a power of two; `CHANNEL_INIT` rejects the channel and stage 'D' can never pass

**File:** `tests/nanokernel/asm/device_exercise.asm:177`

```asm
mov     dword [rbx + 0x2C], 0x1E0000 ; W size
```

The guest builds the channel header at GPA `0x400000`, then commits via the
detcall sequence (`OUT 0xD374/78/7C`, `IN 0xD37C`). The host handler is
`crates/dh-devices/src/detchannel.rs::channel_init`, which calls
`detguest_host::Channel::attach`. That attach path validates **every** ring
descriptor (`../guest-sdk/crates/detguest-host/src/channel.rs:168-174`):

```rust
for ring in RingId::ALL {
    let d = header.ring_desc[ring as usize];
    if d.size == 0 || !d.size.is_power_of_two() {
        return Err(AttachError::BadRingSize { ring });   // ring W hits this
    }
    if d.validate().is_err() { return Err(AttachError::RingOutOfBounds { ring }); }
}
```

`0x1E0000 = 1,966,080` is **not** a power of two, so attach returns
`AttachError::BadRingSize { ring: W }`. `AttachError::init_status()`
(`channel.rs:59-63`) maps that to `InitStatus::BadMagicVersion` (status **2**).
The guest's `IN 0xD37C` therefore reads a nonzero value:

```asm
in      eax, dx          ; status: 0 = attached -> actually reads 2
test    eax, eax
jnz     .fail_d          ; taken -> emits lowercase 'd', parks
```

**Consequence:** the 'D' stage can never succeed. The program emits `CEPBd`
and parks. `DEVICE_EXERCISE_OK_SEQUENCE = b"CEPBDX"` (lib.rs:65) is **unreachable
on a real run**, defeating the entire purpose of the M1-acceptance guest.

**Root cause — a spec self-contradiction the asm transcribed faithfully.** The
asm comment (line 18) and ARCHITECTURE.md §2's layout table both list ring W at
`0x1E0000`. But the authoritative implementation
(`../guest-sdk/crates/detguest-wire/src/header.rs`) sizes ring W at
**`0x10_0000` (1 MiB)** and documents exactly why in the `RING_W_SIZE` doc
comment: the doc's own normative index discipline ("free-running u32 masked by
`size − 1`, sizes are powers of two") plus the attach validation both require a
power of two, and `0x1E0000` violates both; the bytes `0x12_0000..0x20_0000` are
reserved. The implementation explicitly overrides the layout-table number and
flags it as a spec-doc issue. The asm followed the wrong line.

**Fix (one line):**

```asm
mov     dword [rbx + 0x2C], 0x100000 ; W size = 1 MiB (power of two; matches
                                     ; detguest_wire::header::RING_W_SIZE)
```

The other three descriptors are unaffected and remain disjoint and back-to-back
(C `0x8000`/`0x4000`, I `0xC000`/`0x4000`, A `0x10000`/`0x10000`, W
`0x20000`/`0x100000`); the W data region `0x20000..0x120000` still lies inside the
2 MiB page and does not overlap A. The Beacon write at W offset 0
(`0x20000`) and the `ringW_prod` store at `+0x280` are independent of the W size
and need no change.

**Also fix the module-header comment (lines 18 and 24) and any narrative** that
restates ring W as `0x20000/0x1E0000` so the clean-room layout note matches the
implementation, not the contradictory doc table.

---

## IMPORTANT

### I1 — No host-runnable test guards the channel layout; the C1 bug slipped through because nothing attaches the bytes the guest writes

**Files:** `tests/nanokernel/tests/elf_shape.rs`,
`crates/dh-devices/tests/detguest_host_smoke.rs` (existing pattern)

The only coverage added is `elf_shape.rs` (the ELF is a static x86-64 exec at the
load address) and `lib.rs`' non-empty check. Neither exercises the channel header
the guest builds, so a malformed ring descriptor is invisible to CI — the bug is
purely execution-gated and there is no VMM+serial end-to-end harness for this
guest yet.

The repo already has every primitive needed to close this cheaply:
`crates/dh-devices/tests/detguest_host_smoke.rs` uses `MockGuestMem`,
`Channel::attach`, `RecordingSink`, and `drain_events`. A focused test can write
the **exact** header + Beacon bytes the asm produces into a `MockGuestMem` at
`0x400000` and assert:

1. `Channel::attach(gm, 0x400000)` returns `Ok` (would FAIL today — proving C1),
2. after publishing `ringW_prod = 24`, `drain_events` yields exactly one
   `Beacon { beacon_id: 0xB33F }`.

Keeping the asm's magic numbers and that test in lockstep (ideally referencing the
`detguest_wire::header::*` constants) prevents this class of drift permanently.
This is the single most valuable follow-up; see action items.

### Note on `vnanos` semantics (acceptable, no change required)

The Beacon record stores the sampled pv-clock VNS in the `vnanos` field, whereas
API.md §3.0 documents `vnanos` as "guest CLOCK_MONOTONIC_RAW ns". The host drain
(`drain_events`) decodes records by `len`/`kind` and treats `vnanos` as opaque —
it never validates the field's provenance — so a sampled VNS is accepted and does
not cause a drain failure. For a test guest with no SDK runtime, sampling the only
available monotonic source is reasonable. Flagged for awareness only; not a defect.
