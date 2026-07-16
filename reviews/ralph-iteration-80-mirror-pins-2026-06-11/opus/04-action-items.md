# Action Items

### Critical

None.

### Important

- [ ] **Add a source-grep gate forbidding `as i32` casts on `SlotState` (the literal
      half of the iteration-79 sr5 extension).** The extension asked to "grep-forbid
      `as i32` casts on SlotState"; the landed change discharges this by doc convention
      and a local lying-casts pin, but nothing mechanically stops a future module (ol1's
      slot table) from writing `slot.state as i32` into `SlotInfo.state` and silently
      mislabeling run state. Model it on `crates/dh-devices/src/lib.rs:86`
      (`no_host_ambient_authority`): a `#[test]` that walks `dh-worker/src/*.rs`,
      forbids the token `SlotState as i32` / `as i32` adjacent to a `SlotState` binding,
      and allow-lists `proto_map.rs` (the one legitimate mention site, where the cast is
      deliberately applied to the *proto* result inside pins). There are currently zero
      such casts in the tree and ol1 does not yet exist, so this is forward-looking
      hardening — file it as a blocker/acceptance-criterion on ol1, not on this branch.
      Non-blocking for the current merge.

### Suggestions

- [ ] **(S1)** Add an inline comment in `every_proto_stop_reason_fits_the_u8_slot`
      cross-referencing dh-proto's `Faulted as i32 == 7` pin
      (`crates/dh-proto/src/lib.rs:162`) so a maintainer who trips the `seen == 8`
      assertion knows the count is intentionally coupled to the proto enum.

- [ ] **(S2)** Add a one-line comment in `stop_reason_wire_numbers_are_pinned`
      noting it is complementary to (not redundant with) dh-proto's number pins:
      dh-proto pins the proto variant numbers; this pins the domain→proto *routing*
      to those numbers. Prevents a future "deduplication" from deleting the routing pin.

- [ ] **(S3)** File a follow-up bead: the reverse direction (proto → domain) is
      correctly absent today (worker only emits domain→proto), but if an in-repo client
      or a verify-path round-trip ever needs proto→domain, it must be a hand-written
      `Result`-returning match — the same offset/order trap applies in reverse and the
      proto `*_UNSPECIFIED`/`*_S` variants have no domain home.

- [ ] **(S4)** When the run loop seals an END record from a `runctl::SegmentOutcome`,
      add a round-trip test (segment outcome → END byte → `StopReason::try_from` == the
      proto mirror of the domain reason) to close the last seam between
      `stop_reason_to_proto` and the inputlog fixture coupling. Out of scope for sr5;
      attach to the run-loop bead.
