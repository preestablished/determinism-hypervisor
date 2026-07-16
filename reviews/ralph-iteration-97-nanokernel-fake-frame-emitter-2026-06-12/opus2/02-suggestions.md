# Suggestions (non-blocking)

## S1 — Tighten the "fresh-booted guest against a restored device" rationale

**File:** `tests/nanokernel/asm/fake_frames.asm:8-12`; mirrored in
`tests/nanokernel/src/lib.rs:228-233` and `elf_shape.rs:550-554`.

The header argues the device read makes continuity hold "BY CONSTRUCTION for
every composition — including a fresh-booted guest against a restored device
state." That specific composition is the weakest leg of the argument:
`RestoreSnapshot` restores guest registers **and** the pv-pad device section
together (`pad.rs::restore` reloads `frame_counter` from the DHSNAP `PADD`
section, and the guest's `r10d` is restored with the rest of the vCPU state). In
the ordinary lineage the comment itself already concedes a register-tracked `F`
"survives an ordinary restore anyway." So "fresh guest, restored device" is not a
composition the normal restore path produces — it would require a harness that
deliberately boots a *fresh* guest image while *pre-seeding* the device counter
(e.g. a restore-into-fresh-boot or a test fixture that sets `frame_counter`
out-of-band).

The read is still genuinely valuable — but the honest framing is
**defense-in-depth + harness flexibility**, not "every composition the normal
harness produces." Suggest softening to something like: "reading the device
makes continuity hold even under harness compositions where the guest image is
fresh but the device counter was pre-seeded (e.g. a fixture that sets
`frame_counter` directly), so the acceptance need not assume registers carried
`F`." This keeps the rationale (don't remove it) while not overstating what the
v1 restore path does on its own.

## S2 — Half-line note on the u32 wrap non-concern

**File:** `tests/nanokernel/asm/fake_frames.asm:32-33` (the `add r10d, 1` /
`mov [r8 + REG_FRAME], r10d` bump).

`FRAME_COUNTER` is `u32` and strict-increase is the contract. Wrap at `2^32`
frames (~`2e12` instructions) is unreachable in practice, but a one-line comment
("u32 wraps at 2^32 frames ≈ 2e12 instr — unreachable; strict-increase holds
until then") would pre-empt a future reader wondering whether the emitter needs
wrap handling. Optional; purely a reader-friendliness nicety.

## S3 — Consider asserting the boot read precedes the `'G'` OUT, not just `.frame`

**File:** `tests/nanokernel/tests/elf_shape.rs:587-595`.

The pin asserts `read_at < loop_at` (read before `.frame:`). The header's stated
invariant is slightly stronger: the read is the *first* meaningful action and the
`'G'` marker is emitted *after* the read as a boot proof. If a future edit moved
the `'G'` OUT above the read (so the marker no longer proves the read happened),
the current pin would still pass. If the "boot proof comes after the read"
property is load-bearing for the acceptance, consider also asserting
`read_at < position_of('G' OUT)`. If it is *not* load-bearing, ignore this — the
existing `read_at < loop_at` is the property that actually matters for continuity.

## S4 — Stale comment about `and ebx, 511` bounding `work_buf`

**File:** `tests/nanokernel/asm/fake_frames.asm:38-40`.

The inline comment (copied verbatim from `pad_echo`) says the `and ebx, 511`
"bounds work_buf writes if pacing is ever retuned past 512." That is accurate,
but in `fake_frames` the pace loop is the guest's *only* memory writer
(no RAM table, unlike `pad_echo`), so the masking is the sole thing standing
between a retuned `PACE_ITERS > 512` and an out-of-bounds `work_buf` store. Worth
a one-clause addition noting that here it is the *only* bound (in `pad_echo` the
reader's attention is split across the table logic). Minor; the code is correct
as written.
