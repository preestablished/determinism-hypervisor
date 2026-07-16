# Positive Notes

## P1: Fork-PR guard is correct and well-placed

`ci.yaml:49`:

```yaml
if: github.event_name == 'push' || github.event.pull_request.head.repo.full_name == github.repository
```

This is exactly the pattern the security research recommends for a self-hosted runner on a public repo. It correctly allows same-repo pushes and same-repo PRs while excluding fork PRs (which is where the RCE risk lives), and it behaves correctly across re-runs because GitHub preserves the original event context. The inline comment (`ci.yaml:47-48`) explains *why*, which is the right level of documentation for a security-critical line.

## P2: Explicit `/dev/kvm` readability probe with a clear actionable error

`ci.yaml:64`:

```yaml
- run: test -r /dev/kvm || { echo "::error::/dev/kvm not available on runner"; exit 1; }
```

Failing fast with a `::error::` annotation (instead of letting `cargo test` produce a confusing mid-suite failure or — worse — silently self-skip) is the right call. `-r` is the correct permission probe for the runner user that will subsequently `open()` the device.

## P3: Sibling-checkout fix matches the path-dep reality

Adding the `../guest-sdk` checkout (`ci.yaml:27-30` and `59-62`) next to the existing `../control-plane` checkout correctly mirrors the root `Cargo.toml` path deps (`detguest-host = { path = "../guest-sdk/crates/detguest-host" }`, `detguest-wire = { path = "../guest-sdk/..." }`). The checkout `path:` values (`repo`, `control-plane`, `guest-sdk`) produce the sibling layout the `../` deps need. This is the fix that unsticks the currently-red `main`, and the header comment (`ci.yaml:6-8`) honestly flags the "sibling HEAD wins — no rev pinning" coupling, which matches the cargo-workspace research's warning about non-reproducible sibling builds.

## P4: Clippy fixes are all semantics-preserving

All four are genuine warning removals with no behavior change:

- **`agenda.rs:408`** `(rng.next() % 4 != 0)` → `(!rng.next().is_multiple_of(4))`. Equivalent: `n % 4 == 0` ⇔ `n.is_multiple_of(4)` for `u64`, including the `n == 0` case (0 is a multiple of 4). Crucially `rng.next()` is still called exactly once, so the deterministic RNG draw order — which this property test depends on — is unchanged.
- **`agenda.rs:409`** `(rng.next() % 2 == 0)` → `(rng.next().is_multiple_of(2))`. Same reasoning; single RNG draw preserved.
- **`msr.rs:124`** removes `use kvm_ioctls::{MsrExitReason, VcpuExit};` — confirmed neither symbol is referenced anywhere in `msr.rs` after the change (grep: 0 hits). Dead import removal only.
- **`msr.rs:141`** `(*base..*base + *count)` → `*base..*base + *count` — drops redundant parens around a range passed to `flat_map`; identical range, identical iteration.

## P5: Honest, high-signal comments throughout the new workflow

The added comments (`ci.yaml:6-8`, `10-13`, `43-45`, `47-48`) explain the *why* of each lane and the sibling-checkout coupling rather than restating the YAML. This is the kind of CI documentation that survives contact with a future maintainer.
