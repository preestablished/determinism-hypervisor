# Critical and Important findings

## Critical

None.

---

## Important

### I-1 — The documented "MMIO error" failure mode is unreachable; device_exercise triple-faults to `Shutdown` instead, and the two MMIO arms in `run_until_hlt` are dead code

**File:** `tools/dh-cli/src/boot.rs` — `write_page_tables` (lines ~176–192) and `run_until_hlt`
MMIO arms (lines ~282–286); module doc at top of file ("the MMIO hole is NOT mapped — fine for
hello, not for the device-exercise guest").

**What the code claims.** The module header and the `MmioRead/MmioWrite` arms imply that booting
a guest which touches the device window will surface a clean
`UnexpectedExit("MMIO at {gpa:#x} (M0 loop has no device bus)")`. The review prompt asked me to
confirm `device_exercise` "fails with the documented MMIO error."

**What actually happens (live-verified).** It does **not**. `device_exercise`'s first device op is
`mov rax, [0xD0000008]` (CLOCK_BASE+VNS, GPA `0xD000_0008`, inside the MMIO hole). But
`write_page_tables` identity-maps only `[0, mem_bytes)` with 2 MiB pages — the MMIO hole at
`0xD000_0000` is **never** present in the guest page tables. So the guest takes a **page fault** at
the paging layer before KVM ever generates an MMIO exit; with no IDT the fault escalates to a
triple fault, and KVM_RUN returns `VcpuExit::Shutdown`:

```
$ ./target/debug/dh-cli boot device_exercise.elf
dh-cli boot: unexpected exit: Shutdown
exit=1
```

This is **not** fixable by giving more RAM: `boot()` caps `mem_bytes` at 1 GiB
(`0x4000_0000`) and `create_slot_vm` caps at `MMIO_HOLE_BASE` (`0xD000_0000`); either way RAM never
reaches `0xD000_0000`, so the identity map can never cover the hole. I confirmed at `--mem-mib 1024`:
still `Shutdown`.

**Consequence.** The `MmioRead`/`MmioWrite` arms of the `classify_exit` match in `run_until_hlt`
are **dead code** for any guest whose page tables only map RAM (which is every guest this loader
builds page tables for). A maintainer reading the code will believe device accesses produce a
labeled MMIO error; they produce an unlabeled `Shutdown`. The behavior is still a *correct
failure* for M0 (M0 has no device bus, so the guest *should* fail), so this is Important-not-
Critical — but the divergence between documented and actual behavior is a real trap.

**Recommended fix (pick one):**
- **(a) Honest comment + keep the arms as defense-in-depth.** Change the module doc and add an
  inline note on the MMIO arms: "Unreachable in M0 because the identity map never covers the MMIO
  hole — a device touch page-faults to `Shutdown` first. Kept for the s0p loader, which *will* map
  the hole as a no-memslot region." That is the cheapest correct fix.
- **(b) Map the MMIO hole as a not-present-but-classified region.** Out of scope for M0; do not do
  this here.
- Either way, **adjust the `Shutdown` message** so the operator gets a hint: today `Shutdown` is
  classified as the generic `ev => UnexpectedExit(format!("{ev:?}"))` arm, printing only
  `unexpected exit: Shutdown`. For a guest that touched a device, that is opaque. Consider a note
  in the `Shutdown` case like "(triple fault — did the guest touch the device window / unmapped
  RAM?)".

This finding also means the review prompt's stated expectation ("boot device_exercise and confirm
it fails with the documented MMIO error") is **false as written** — worth recording so s0p does not
inherit the assumption.

---

### I-2 — `boot.rs`'s `IoIn` arm bypasses `classify_exit` entirely and re-implements RAZ for *all* IN ports, diverging from the shared IN-FILL contract

**File:** `tools/dh-cli/src/boot.rs` — `run_until_hlt`, line ~240:
`VcpuExit::IoIn(_port, data) => data.fill(0),`

**Observation.** The `IoIn` arm is matched **before** the catch-all `other => classify_exit(other)`
arm, so every PIO IN — serial, detcall window, and any other port — is answered with `fill(0)` here
and `classify_exit` never sees an IN. That means:

1. The carefully-specified IN-FILL contract in `classify_exit` (`DetcallIn`/`SerialIn` returning
   *unfilled* buffers so the caller writes deterministic replies) is **never exercised** by this
   path. boot.rs is a parallel, simplified implementation of IN handling, not a consumer of the
   shared dispatch. The header comment ("the classify_exit IN-FILL contract — INs are answered HERE
   on the raw exit, before classify_exit ever sees them") is accurate about *what* it does, but the
   net effect is that the two modules now have two independent definitions of "what an IN returns,"
   which can silently drift.

2. **Future serial-device hazard (flag for the serial-device bead, avm).** Because the arm has no
   port guard, a future 16550 driver polling the Line Status Register at `0x3FD` (LSR, `THRE`/`TEMT`
   bits) would read `0x00` forever → "transmitter never ready" → the driver spins. The current
   guests are safe: `hello`/`landing_loop` use **blind `out`** with no status poll (verified in
   `hello.asm`/`landing_loop.asm`), so no IN to `0x3FD` ever happens. This is *fine for M0* but is a
   latent trap the moment a guest does proper 16550 handshaking. The `SERIAL_BASE..SERIAL_END`
   constants already exist in `boot.rs` for the OUT arm but are unused on the IN side.

**Recommended fix (low cost, M0-appropriate):** Either (a) route INs through `classify_exit` and
honor its returned events (the "right" long-term shape), or (b) at minimum leave a comment on the
`IoIn` arm: "M0 RAZ-fills *every* IN port, including the serial LSR (`0x3FD`); a status-polling
16550 driver would spin — the serial-device bead (avm) must model LSR before any guest polls it."
I lean (b) for M0 to keep the loop dumb, with the comment as the guardrail.
