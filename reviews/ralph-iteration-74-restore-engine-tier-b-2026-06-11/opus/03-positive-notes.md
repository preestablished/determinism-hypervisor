# Positive Notes — patterns to preserve

## P1 — Fail-closed everywhere; no silent skips

Every mismatch is a loud typed error. The LAPC placeholder is checked
*positively* — `lapc.sec_version != 1 || !lapc.contents.is_empty()` rejects a newer
writer's stub rather than ignoring it (`restore_engine.rs:196-206`), with the
comment correctly framing a silently-dropped interrupt-state section as a
determinism bug. The bidirectional shape check (`:227-255`) makes "snapshot taken
on a differently-shaped machine" unrepresentable as a successful restore. This is
exactly the strictness the design demands and the review brief asks be preserved.

## P2 — The downcast seam is minimal, justified, and well-documented

`DetDevice::as_any_mut` defaults to `None` (`dh-devices/src/lib.rs:58-66`) and is
overridden only by `PvClock` (`clock.rs:179-183`). The doc comments on both the
trait method and the engine call site (`restore_engine.rs:22-28`, `:257-268`)
explain *why* the seam exists: `vns_base` is engine-supplied state that lives in
TIME, not in CLKD, because a segment's own base would be stale by construction.
The engine fails loudly if the downcast does not land
(`Codec("clock device does not downcast to PvClock")`, `:263-265`) rather than
silently skipping the re-seed. This is the right shape for a downcast: opt-in,
defaulted-off, named in exactly one place, and non-silent on failure. I considered
alternatives (a dedicated `set_engine_state(&TimeSection)` trait method, or a typed
`DetClock` sub-trait) and concluded the `Any` seam is the least-invasive choice
for a single consumer; revisit only if a second device ever needs engine-supplied
state.

## P3 — The capture/restore mirror is faithful

`restore_snapshot` reads as a precise inverse of `take_snapshot`: same fixed
section set, same ENTR v2 version-domain split (write `device.restore(&regs, 1)`
at the device version, never the section's `2` — `:217-224`, mirroring
`snapshot_engine.rs:249-256` and matching the explicit warning in
`dhsnap.rs:407-418`), same MCFG identity check via `canonical_encode`
(`:179-190`). Keeping the two engines structurally parallel makes the
fixed-point property auditable by eye.

## P4 — The transparency tests are real, not tautological

`full_restore_resnapshot_yields_the_identical_ref` (tests `:247-314`) and the
delta variant (`:316-470`) drive the *real* in-process snapshot-store, restore into
a genuinely fresh slot + default-state bus, re-snapshot, and assert ref-equality —
which, because the ref is the store's content hash of the container
(`client.rs:846-875`), is a byte-identical-container assertion. The delta test
additionally executes real guest instructions (3 `mov` + `hlt`,
tests `:357-383`) through `vcpu.run()` to dirty pages, harvests the dirty ring,
takes a DELTA, and proves the server-flattened chain materializes the full state
(`:438-449`) — exercising the "engine never walks parents itself" contract. None of
these re-implement production logic; they assert the documented contract.

## P5 — Strong negative-case suite covering the documented error variants

`restore_preconditions_and_mismatches_fail_loudly` (tests `:472-572`) and
`mis_shaped_containers_are_rejected_loudly` (`:589-712`) reach `NotPaused`
(all three non-Paused states), `Store` (unknown ref), `ConfigMismatch` (kernel-hash
*and* RAM-size variants), and `Codec` (wrong blob format, unparseable DHSNAP,
non-empty LAPC, missing ENTR, extra section, missing device section). The
`rebuild` helper (`:574-587`) re-frames a real captured container with a single
mutation — a clean way to craft adversarial-but-otherwise-valid inputs. This
matches the integration-testing research note's emphasis on reaching every
documented error variant and using `assert!(matches!(...))` for enum-shape
assertions.

## P6 — Honest scrap-slot error contract

The function-level doc (`restore_engine.rs:88-91`) and the `Store` variant doc
(`:67-69`) state plainly that on error the slot's RAM may be partially written and
the caller must discard it. The engine never tries to look healthy after a partial
write — it returns the typed error and stops. The `NotPaused` and `ConfigMismatch`
(RAM-size, MCFG) checks all fire *before* any page lands (`:103-130`,
`:179-190`), so the common precondition failures leave the slot untouched; only
deeper failures hit the scrap path. This honesty is the right call.

## P7 — `devices_mut` preserves the deterministic base order

`bus.rs:126-132` documents and preserves the same sorted-base iteration order as
`devices()`, so the restore pass visits devices in the same deterministic order as
capture — no dependence on hash-map iteration or insertion order.
