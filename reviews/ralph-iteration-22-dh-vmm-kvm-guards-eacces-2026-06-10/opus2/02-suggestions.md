# Suggestions (non-blocking)

### S-1 — Collapse the `kvm_available()` → `kvm_usable()` double-indirection

Each of the three `dh-vmm` test modules keeps a local helper that does nothing
but forward:

```rust
fn kvm_available() -> bool {
    crate::kvm::kvm_usable()
}
```

(`kvm.rs:418-420`, `msr.rs:126-128`, `run.rs:138-140`.)

Two names for one predicate is a readability cost with no payoff. A reader now
has to chase `kvm_available` → `kvm_usable` to learn that "available" actually
means "rw-openable", and the indirection invites future drift (someone "fixes"
one module's `kvm_available` body and the three diverge). Options:

- **Best:** delete the three forwarders and call `crate::kvm::kvm_usable()`
  directly at each `if !...` guard. One name, one definition.
- If a module-local alias is preferred for brevity, `use crate::kvm::kvm_usable
  as kvm_available;` expresses "this is just an alias" far more honestly than a
  hand-written forwarding fn.

Either way, settle on **one** name. The condition is "can we rw-open /dev/kvm",
so `kvm_usable` reads better than `kvm_available` (existence ≠ usability is the
whole lesson of this iteration); consider renaming the guard sites to `kvm_usable`
and dropping `available` entirely.

---

### S-2 — Fix the stale `dh-vmm` skip messages

Independent of whether I-2's de-duplication lands, the eight live `dh-vmm` guards
still print `eprintln!("skipping: no /dev/kvm")`
(`kvm.rs:425,439,460,476`; `msr.rs:169,214,263`; `run.rs:157,215`). Under the new
access-probe this is factually wrong on hosted CI — the node exists, access was
denied. Update to match `dh-worker`'s accurate
`"skipping: /dev/kvm not usable"` so logs do not mislead whoever debugs a
"why did the live test skip" question in CI.

---

### S-3 — Add one assertion-free comment distinguishing "usable" from "compliant"

`kvm_usable()` answers "can I open the device", not "is this a §7.4/§2.1
compliant host". The live tests then call `KvmSystem::open()` /
`create_slot_vm()` which assert real capabilities. That layering is correct, but
the `kvm_usable` doc comment frames the rw-open as the gate for "run only where
the box is §7.4-usable" (preflight) — slightly conflating *openable* with
*compliant*. A one-line note ("openable, not necessarily compliant — the live
asserts below carry compliance") would prevent a future reader from assuming the
probe vets more than it does.

---

### S-4 — Consider an explicit ignore/skip mechanism over silent early-return

This predates the change, but the iteration touches every site so it is worth
flagging: the guards use `if !kvm_usable() { eprintln!(...); return; }`, which
reports a **passed** test that did nothing. A skipped live test and a real
passing live test are indistinguishable in `test result: ok`. If the project ever
wants CI to assert "the kvm-intel lane actually ran N live legs" (the natural
defense against I-1's silent-skip risk), an env-gated hard-fail
(`DH_REQUIRE_KVM=1` → `panic!` instead of `return` when the probe fails) or
moving the live legs behind `#[ignore]` + an explicit `--ignored` lane would make
"did the live coverage run" observable rather than implicit. Not required for this
change; file as a follow-up if coverage-visibility matters.
