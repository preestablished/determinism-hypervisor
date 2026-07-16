# Critical & Important

**No Critical findings.** **No Important findings in d2p's scope** — the in-tree code is
correct and well-tested. The two items below are Important *cross-bead* reconciliations:
the code's behavior is right, but it diverges from spec text the fork beads will read.

---

## The headline experiment (resolves the 9e4 risk — NOT a defect, a green light)

The prompt's central worry: if `F_SEAL_FUTURE_WRITE` also blocked `MAP_PRIVATE`
CoW-writable mappings, the tier-A fork design (9e4) would be dead on arrival. I probed
the actual 6.8 kernel behavior:

```
parent[0]=0xab (pre-seal write ok)
seals after  = 0x16 (FUTURE_WRITE=0x10 | SHRINK=0x02 | GROW=0x04)
RESULT:  mmap(MAP_PRIVATE, PROT_READ|PROT_WRITE)  => OK (cow[0]=0xab)
         CoW write ok: cow[0]=0x11  parent[0]=0xab  (parent untouched — CoW works)
CONTROL: mmap(MAP_SHARED,  PROT_READ|PROT_WRITE)  => EPERM  (new writable shared denied)
CONTROL: mmap(MAP_SHARED,  PROT_READ)             => OK     (children read baseline)
EXTRA:   mprotect(RO-shared -> RW)                => EACCES (can't escalate a shared RO map)
```

**Implication for 9e4 (flag either way, as requested): GREEN.** The kernel semantics of
`F_SEAL_FUTURE_WRITE` are exactly what the design assumes:

- A private (CoW) writable mapping of the sealed parent memfd is **permitted** — the
  child gets its own anonymous CoW pages on first write, parent memory stays intact.
- A *shared* writable mapping is **denied** (EPERM) — so no second process can scribble
  the shared baseline.
- `mprotect` cannot launder a RO *shared* mapping into RW (EACCES) — the seal isn't
  bypassable that way.

This matches `man F_SEAL_FUTURE_WRITE`: it only blocks `PROT_WRITE|MAP_SHARED` (and
existing-mapping write escalation); `MAP_PRIVATE` is by design unaffected because CoW
writes never touch the file. **9e4's tier-A CoW path is sound. No design change needed.**
Probe source preserved at `/tmp/sealprobe.c` for reproduction.

---

## Important (cross-bead — spec reconciliation, file against 9e4 / fork epic)

### I-1. `Faulted` slot state exists in the proto but not in the Rust state machine

- **Spec:** `.agents/docs/determinism-hypervisor/API.md:441-442` —
  `enum SlotState { ... FROZEN = 4; FAULTED_S = 5; }`.
- **Code:** `crates/dh-vmm/src/lib.rs:33-41` — `enum SlotState { Empty, Running, Paused,
  Frozen }`. No `Faulted`.
- **Why it matters:** `ensure_write_path` (lib.rs:104) is `match self { ... }` over four
  variants — *exhaustive today*. The moment `Faulted` is added (it must be, to honor the
  proto), this match and `can_transition`'s tuple list silently won't cover it, and the
  compiler will force a decision — good — but the **transition relation will be wrong by
  omission**: there is no `* → Faulted` edge anywhere, so a guest-contract violation has
  no legal state to land in. A faulted slot is also a write-path denial (like Frozen),
  which `ensure_write_path` currently can't express.
- **Action:** Not d2p's job to implement Faulted, but the d2p author should (a) leave a
  `// TODO(fault-path): Faulted state — see API.md SlotState::FAULTED_S` next to the enum
  so it isn't forgotten, and (b) the fork/fault bead must add `Running → Faulted`,
  `Paused → Faulted`, `Faulted → Empty` edges and a `FaultedWriteDenied` arm.

### I-2. ARCHITECTURE lifecycle line shows `Running → Frozen`; the code (correctly) rejects it

- **Spec (stale):** `.agents/docs/determinism-hypervisor/ARCHITECTURE.md:740-741` —
  `Empty → Created (...) → Paused ⇄ Running → Frozen (...) → Empty`. Read literally, that
  arrow chain implies `Running → Frozen`.
- **Spec (authoritative, agrees with code):** ARCHITECTURE.md §8.4:690-699 — freeze
  happens *"once the parent **pauses**"*. So fork requires a Paused parent.
- **Code:** lib.rs:78-90 allows only `Paused → Frozen`, and the test
  `running_cannot_be_destroyed_or_frozen_directly` (lib.rs:233-237) asserts
  `!Running.can_transition(Frozen)` with the comment "fork requires Paused parent."
- **Verdict:** **The code is right** — §8.4 is the binding text and the EBUSY/F_SEAL
  reasoning depends on the parent having paused. The lifecycle one-liner at :740 is just
  a loose summary that elides the intermediate pause. **Fix the doc line** (or add "(via
  Paused)") so the fork-bead author doesn't trust the wrong arrow. No code change.

---

## Skeptic angles explicitly cleared (so the next reviewer doesn't re-chase them)

- **Re-restore / Paused→Paused:** Not needed. ARCHITECTURE.md:740 lists RestoreSnapshot
  among `Empty → Created` operations, and API.md returns a fresh `Lease`. Restore
  allocates a **fresh slot**, so `Empty → Paused` covers it; no `Paused → Paused` edge
  required. Correct.
- **Unfreeze + irrevocable seals soundness:** Sound. `Frozen → Paused` (last child freed)
  returns the parent to Paused; its **existing** KVM mapping stays writable (seals are
  FUTURE-only), so KVM_RUN works and the slot can Run again. The only thing that would
  EPERM is establishing a *new writable shared* mapping — which neither re-run nor the
  next fork generation needs (the next fork is another `MAP_PRIVATE` CoW, which my probe
  proved still works even after sealing). Parent-destroy is also fine (`Frozen → Empty`).
  **No reuse pattern breaks.** The one theoretical hazard — re-restore machinery wanting a
  fresh *writable shared* mapping of the same memfd — doesn't apply because restore
  allocates a fresh slot (above), not a re-map of the sealed one.
- **Test isolation:** Clean. Each test calls `sys.create_slot_vm(...)` → fresh memfd, so
  the pre-freeze `assert_eq!(seals & F_SEAL_FUTURE_WRITE, 0)` can't be polluted by another
  test. `memfd_create` defaults have no seals (probe shows `seals before = 0x0`), and
  `F_SEAL_SEAL` is deliberately not added so re-freeze stays a no-op (asserted at
  kvm.rs:546). Confirmed by running `freeze_ram` in isolation — passes.
