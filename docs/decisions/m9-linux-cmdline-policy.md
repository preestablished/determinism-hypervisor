# Decision: M9 Linux cmdline canonicalization

**Bead:** determinism-hypervisor-4s9.9 · **Status:** decided 2026-06-18 ·
**Owner mechanism:** `crates/dh-vmm/src/config.rs` +
`crates/dh-worker/src/proto_map.rs` + `proto/hypervisor.proto`

## Context

Linux direct boot adds a guest-visible command line that is also part of
`MachineConfig` identity. If callers can supply the whole command line, they can
silently remove deterministic boot controls or create multiple byte spellings for
the same logical request. M9 therefore needs one hypervisor-owned baseline and a
small append-only extras surface before config hashing and before the loader
builds Linux boot params.

## Decision

The forced M9 Linux baseline command line is exactly these bytes:

```text
console=ttyS0 nokaslr norandmaps random.trust_cpu=off tsc=unstable clocksource=dh-pvclock nohz=off highres=off init=/init
```

`BzImageBoot.cmdline` is interpreted as append-only extras, not as a complete
guest command line. The accepted extras are:

- `quiet`
- `loglevel=<n>`, where `<n>` is a single decimal Linux loglevel in `0..=7`

No other token is accepted for M9. Any duplicate token, duplicate key, attempt
to override a forced baseline key, empty token, non-ASCII byte, embedded NUL, or
unsupported token is a config error before `MachineConfig` hashing.

Canonicalization emits:

1. The forced baseline bytes above, in the exact order and spelling shown.
2. If present, one ASCII space and `quiet`.
3. If present, one ASCII space and `loglevel=<n>`.

There is no leading whitespace, repeated whitespace, or trailing whitespace in
the canonical byte string. Input extras may contain ASCII whitespace between
tokens, but canonical output always uses one ASCII space. The canonical output
order is the whitelist order above, regardless of caller input order.

`MachineConfig::config_hash` snapshots these canonical bytes for
`BootSpec::BzImage`. The Linux boot-params cmdline pointer exposes the same
canonical bytes to the guest; the loader may add the required NUL terminator in
guest memory, but the terminator is not part of the `MachineConfig` cmdline
bytes.

## Consequences

The proto surface remains compact: callers can request quiet boot or a specific
kernel loglevel without taking ownership of deterministic baseline controls.

Any future extra must be added to this decision or a superseding decision before
it is accepted by `proto_map` or included in `MachineConfig` hashing.

Tests for M9 must assert the canonical byte string, duplicate rejection,
unsupported-token rejection, config-hash sensitivity, and that Linux observes the
same bytes through boot params.
