# Critical & Important Findings

## Critical

None. The implementation is sound for the M3 run-twice-compare scope.

---

## Important

### I-1. No `from_value` constructor — a restored chain cannot resume (M4 blocker, cheap to land now)

`StateHashChain` (hash.rs:60-141) exposes only `new(machine_config_hash, base_snapshot_ref)` (H_0)
and `value()`. There is no way to **reconstruct a chain from a stored value**.

ARCH §8.1 explicitly stores `hashchain: current chain value + epoch index` inside the snapshot, and
§8.3 restore step 3 says "restore device models, PRNG, **hashchain**". A restored or forked guest must
resume hashing from the parent's chain value — `H_{i+1} = blake3(H_i || ...)` where `H_i` is the
*restored* value, not a fresh `H_0`. With only `new()`, restore is forced to either re-derive H_0
(wrong: drops the execution-history-prefix property §8.5 calls the whole point) or reach into the
private `value` field (the field is private; nothing outside the module can set it).

The module docstring even foregrounds the chain-as-history semantics ("comparing chains compares
execution histories"), which restore cannot preserve without a seed constructor.

**Why now, not "M4 owns it":** the bead text says M4 *extends rather than replaces* this module. A
restore path that cannot seed the chain is a replace, not an extend — adding the constructor now keeps
the M4 promise honest and costs three lines. Recommend:

```rust
/// Resume a chain from a restored/forked value (§8.1 hashchain field, §8.3 restore).
pub fn from_value(value: [u8; 32]) -> Self {
    StateHashChain { value }
}
```

(Severity is Important rather than Suggestion specifically because the bead's "extends not replaces"
contract is a stated acceptance property, and this is the one API gap that forces a replace.)

### I-2. `device_sections()` is unused and untested — the one variable-length region in the preimage has zero coverage

`pub fn device_sections(bus: &dh_devices::MmioBus) -> Vec<u8>` (hash.rs:308-319) is the **only**
producer of the variable-length middle region of every link's preimage, and it is:

- **not called anywhere** outside hash.rs (grep of `crates/` for `device_sections`, `push_link`,
  `push_final_link`, `StateHashChain` returns no hits outside the module), and
- **not covered by any test** — the live `final_link_sees_guest_ram_live` test passes `b""` for
  `device_sections`, and the unit tests pass `b"devs"` literals. The framing logic
  (`device_id || section_version || len || bytes`, base order) is never exercised.

This matters because device_sections is precisely where preimage ambiguity (see S-1) and the
determinism of bus iteration order would surface. `MmioBus::devices()` (bus.rs:122) documents
base-order iteration and `DetDevice::snapshot` (lib.rs:53) is contractually "a pure function of device
state" — but the *framing* code in `device_sections` (the `len as u32` cast, the id/version prefix
ordering) has no test asserting it round-trips deterministically or that two equal buses frame equal.
Recommend a unit test building a small `MmioBus` with two devices and asserting `device_sections`
output is deterministic and order-stable, plus at least one `push_link` test that feeds non-empty
device bytes. Without a caller, this is also dead code that could silently rot before M4 wires it in.
