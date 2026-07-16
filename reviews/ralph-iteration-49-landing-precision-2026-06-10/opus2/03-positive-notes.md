# Positive notes

- **The central claim survives an independent attack.** The residue→rip
  analysis (01 §A) confirms landings are at instruction starts on *both*
  guests, and that `rcx==64` occurs at exactly the REP-MOVSB address and
  nowhere else. The test's cross-boot-equality assertion is therefore not
  resting on an unverified assumption — the boundary really is the
  instruction boundary the spec (§3.2) requires.

- **The REP guest is a genuinely clever oracle.** Encoding the mid-REP
  invariant *into the guest's own architectural state* (RCX = 64 only at
  the REP start) means the test needs no external disassembly to detect a
  §3.2 violation — `rcx ∉ {0,64}` is a self-describing failure. This is a
  much stronger check than "RIP didn't change," and it caught my eye as
  the right way to test a property that is otherwise hard to observe.

- **Margin-independence is proven on real targets, not asserted.** Running
  the prefix at the production margin (8192) on boot A and the *same*
  targets at 128 on boot B, then asserting full `Vec<Boundary>` equality,
  turns the §3.2 "landed boundary is independent of the margins" contract
  into a live, executable proof. That's a sharp test design.

- **Determinism is real, not nominal.** No host randomness on the test
  path — SplitMix64 with fixed, date-tagged seeds and a `BTreeSet` for
  distinct, sorted targets. The targets are reproducible across machines.

- **KickGuard re-registration is correctly cheap.** I checked the angle-2
  concern (10k `KickGuard::register` calls): `register` is a single
  thread-local store + struct construction, and `Drop` clears the TLS
  slot. No fd open, no `sigaction`, no per-call signal registration —
  zero leak surface. Re-registering per `land_at` is idempotent by
  construction.

- **DF is clear before the REP runs.** crt0.asm issues `cld` (line 21)
  before `call prog_main`, so `rep movsb` copies forward as intended; the
  src/dst layout (src in .data filled with 0x5A, dst in .bss, both
  `align 64`) is correct and the elf builds/links clean.

- **Loud-by-default failure posture.** Every unexpected exit is a hard
  error (`BoundaryError::Exit`), Overshoot is fatal and never absorbed,
  and the coverage floor (`rep_starts > 50`) guards against a silently
  trivial pass. The test cannot quietly degrade into a no-op.

- **Clean on both arches.** `clippy --workspace --all-targets` is warning-
  free on x86_64 and on aarch64; the `#![cfg(target_arch = "x86_64")]`
  gate correctly compiles the whole target to empty on aarch64, matching
  the documented intent (bead v5w).

- **Excellent self-documenting comments.** The file header's MARGINS
  rationale and the inline justification for each assertion make the
  intent and the cost model explicit — a maintainer six months out will
  understand *why* the prefix exists.
