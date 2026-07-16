# Positive Notes

## P1 — The "4 of 5 casts lie" pin is the standout

`slot_state_wire_numbers_are_pinned` does not just assert the conversion is correct —
it computes `lying_casts = pins.iter().filter(|(d, w)| (*d as i32) != *w).count()` and
pins it at 4, with a comment explaining that the cast agrees on `Paused=2` by pure
coincidence "which is exactly what makes the bug class survive spot checks." This is a
rare test: it encodes *why the safe path exists* and makes the dangerous shortcut
visibly wrong in the same place a maintainer would reach for it. If a future
renumbering ever made the naive cast accidentally correct (or correct for a different
count), this assertion fires and forces a re-examination. Excellent defensive design.

## P2 — Architecture gating is exactly right

`dh_vmm::runctl` is `#[cfg(target_arch = "x86_64")]` (`dh-vmm/src/lib.rs:34`), so
`stop_reason_to_proto` and its test — which name `dh_vmm::runctl::StopReason` — are
correctly x86-gated. `dh_vmm::SlotState` is ungated (`dh-vmm/src/lib.rs:42`), so
`slot_state_to_proto` and the module itself are correctly left ungated. The two
module-level `use` statements (`dh_proto::v1`, `dh_vmm::SlotState`) are both consumed
by the ungated fn, so there is no unused-import warning on aarch64. I verified this
empirically: `cargo clippy -p dh-worker --target aarch64-unknown-linux-gnu --lib` is
clean. This is the subtle part of the change and it was gotten right.

## P3 — Placement in dh-worker is the correct owner

`proto_map.rs` lives in `dh-worker`, the gRPC bridge owner that already depends on
both `dh-proto` and `dh-vmm` (`dh-worker/Cargo.toml:11,14`). This keeps the proto
dependency out of the pure domain crate (`dh-vmm`) and out of the codec crate, and
puts the conversion where its only consumer (ol1's slot serving) will live. No new
dependency edge was needed in dh-worker for this.

## P4 — Exhaustive matches turn future variant growth into a compile error, not a silent fallthrough

Both conversion fns use exhaustive `match` with no `_ =>` arm. The module doc spells
out the contract: when runctl gains `Faulted` / `NextSdkEvent` producers, the match
stops compiling and forces the mapping decision at that commit. I confirmed runctl's
`StopReason` today has exactly 5 variants (no `Faulted`, no `NextSdkEvent` —
`dh-vmm/src/runctl.rs:48-55`), matching the doc's claim. This is the right way to make
"add the arm later" unforgettable.

## P5 — The inputlog mirror test makes a documentation claim mechanically true

API.md §3.3 (line 575 of `.agents/docs/determinism-hypervisor/API.md`) literally says
the END record's `stop_reason: u8` "mirrors proto StopReason." Before this change that
was prose. `golden_fixture_stop_reasons_decode_to_the_intended_proto_variants` decodes
the frozen fixture bytes through the real `LogReader::end()` and asserts the proto
variant — so the doc claim is now a checked fact that breaks the build if either side
renumbers. This is the exact "comment → contract" upgrade the bead asked for, and it
respects the codec's design (`reader.rs:460` deliberately does *not* range-check the
byte; the test lives outside the codec so the carrier stays transparent).

## P6 — The dev-dep choice is appropriate and was sanctioned by the bead

Pulling `dh-proto` (and its tonic/prost chain) into `dh-inputlog` as a **dev-dependency**
keeps the heavy gRPC stack out of the production build of the codec crate while still
allowing the cross-crate coupling test. The bead's own description suggested exactly
this ("Cheap test in tests/determinism or dh-inputlog dev-dep on dh-proto"). The
`Cargo.toml` comment explaining *why* the dev-dep exists is a nice touch for the next
person who wonders why a byte-level codec crate knows about proto.
