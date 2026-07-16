# Positive notes

These are the things the iteration got right, verified by running code and cross-checking
authoritative sources — recorded so a fix pass doesn't accidentally regress them.

### P1 — The layout-table change is exactly right, row-by-row

Verified every row of `.agents/docs/guest-sdk/ARCHITECTURE.md` lines 147–155 against the
authoritative `../guest-sdk/crates/detguest-wire/src/header.rs` constants and its compile-time
`const _` invariant block (lines 110–127):

| Row | Doc | header.rs constant | Match |
|---|---|---|---|
| ring C @ 0x008000, 0x4000 | ✓ | `OFF_RING_C_DATA`/`RING_C_SIZE` | ✓ |
| ring I @ 0x00C000, 0x4000 | ✓ | `OFF_RING_I_DATA`/`RING_I_SIZE` | ✓ |
| ring A @ 0x010000, 0x10000 | ✓ | `OFF_RING_A_DATA`/`RING_A_SIZE` | ✓ |
| **ring W @ 0x020000, 0x100000** | ✓ | `OFF_RING_W_DATA`/`RING_W_SIZE = 0x10_0000` | ✓ |
| **reserved @ 0x120000** | ✓ | `OFF_RESERVED_TAIL = 0x20000 + 0x100000` | ✓ |
| end @ 0x200000 | ✓ | `CHANNEL_SIZE = 0x20_0000` | ✓ |

The `0x1E0000` (1,966,080) figure is gone from the vendored copy, and the new "reserved (unused
page tail; ring sizes are powers of two)" row both fills the table to `end` and states the
*reason* for the gap (0x120000..0x200000 headroom). The `DEVICE_EXERCISE_RING_DESCS` constant
(lib.rs line 156, W = `(0x2_0000, 0x10_0000)`) matches, and `channel_interop` (which attaches a
page built from these values through real detguest-host code) passes live.

### P2 — No stray `0x1E0000` / `1,966,080` left in any non-review doc or code

Grepped the whole repo (excluding `reviews/` and `.git/`): the only remaining hits are
intentional, correct references to the *old* value as history — `device_exercise.asm` line 171
("PRINTED 0x1E0000, but … powers of two"), `channel_interop.rs` line 7, and lib.rs line 150. The
vendored guest-sdk `API.md`/`INTEGRATION.md`/`IMPLEMENTATION-PLAN.md`/`README.md` carry no copy of
the refuted number. The `device_exercise.asm` tense fix (`prints` → `PRINTED`) is a nice touch now
that the table no longer prints it.

### P3 — The §3.1 "retire zero" rewrite is faithful to the empirics and the code

`COUNTING_DELTA_AT_OUT_EXITS = REGION (1000) − EXIT_INSTRS (3) = 997` (lib.rs lines 134–135), the
boundary-engine "never retiring" strengthening, and the cross-reference to the counting constant
all line up with bead 0sc / d34. The mechanism explanation ("exit before retirement, KVM completes
host-side by skipping RIP, exclude_host=1 never sees it") is technically correct.

### P4 — The §6.2 absolute-vns core claim is correct

`TIMER_DEADLINE` genuinely is absolute guest vns on the same axis as `VNS_LO/HI` — confirmed in
`clock.rs`. The "never segment-relative" assertion and the §6.4 `at_frame` parallel are apt; the
only nit is the *where*-it-subtracts phrasing (see 02/S1), not the absolute-vs-relative substance.

### P5 — Build/test hygiene: comment-only edits broke nothing, proven live

Forced nasm rebuild of both edited `.asm` files, ran clippy clean, and ran the full workspace
suite including the heavy live KVM tests (`timer_determinism` 100 runs zero-divergence,
`counting_smoke`, `channel_interop`, `if0_deferral`, `regression`, `boot_hello`). All green. A
documentation-only iteration that nonetheless re-verifies the live ELFs still assemble and the
determinism gates still hold is exactly the right discipline for normative-doc changes.
