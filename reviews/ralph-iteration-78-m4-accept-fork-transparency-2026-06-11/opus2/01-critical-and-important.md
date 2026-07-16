# Critical & Important

## Critical

None.

## Important

### I1 — The timer guest's behavior now silently depends on the first cmdline byte; a future `ITERS_CMDLINE` change could mode-flip it and gut the test without failing loudly

`boot()` (m4_transparency.rs:105) passes `ITERS_CMDLINE` to `load_and_enter`
for *every* guest. For the timer guest this is a new coupling: the prior
timer-guest tests booted it with `b""` (`tests/determinism/tests/timer_determinism.rs:27`)
or an explicit mode string `b"defer"` (`if0_deferral.rs:23`).

`tests/nanokernel/asm/timer_guest.asm` selects its mode from the FIRST
cmdline byte (lines 66–78):

```asm
    movzx   eax, byte [rsi + BOOTINFO_OFF_CMDLINE]
    cmp     al, 'm'   ; -> .masked      (IF never set; injections defer)
    cmp     al, 'a'   ; -> .arm_mode    (needs the device-bus run loop;
                      ;                   an MMIO read is a LOUD foreign exit)
    cmp     al, 'd'   ; -> .defer_mode  (fixed masked window, then open)
    ; else fall through to .open_window (sti + spin) == empty-cmdline path
```

Today `ITERS_CMDLINE = b"30000000"` → first byte `'3'` → none match → the
default open-window path, byte-identical to the empty-cmdline behavior the
guest was designed and previously tested against. So **the test is correct
as written.** This is not a bug.

The risk is maintainability/silence: the value `"30000000"` was chosen as a
*landing-loop iteration count* (the const doc and the `8*iters` comment make
that explicit), and the timer guest is along for the ride purely because its
first byte happens to be a digit. If someone later retunes the landing-loop
budget to, say, `b"mask..."`-shaped or `b"a..."`-shaped — or more plausibly
adds a future timer-guest mode keyed on a digit — the timer test would
**silently switch modes**:

- A leading `'m'` → `.masked`: IF stays 0, so the three scheduled
  injections would defer/never deliver; `injections_delivered` could drop to
  0 and the `assert_eq!(out_a.injections_delivered, 3, ...)` would catch it —
  good. But a mode that *still* delivers all three by a different path could
  pass `A==B` while no longer testing what the test claims.
- A leading `'a'` → `.arm_mode`: the guest does a pv-clock MMIO read, which
  the harness's `on_exit` turns into `BoundaryError::Exit("unexpected exit")`
  → the run panics. Loud, but a confusing failure mode pointing at runctl,
  not at the cmdline choice.

**Recommendation (pick one):**
1. Boot the timer guest with an explicit, intent-revealing cmdline that the
   guest is documented to treat as default — e.g. its own `const` rather
   than reusing the landing-loop `ITERS_CMDLINE`. The cleanest is to let
   `boot()` take the cmdline (or default it) so the timer test passes
   `b""` exactly as the other timer tests do, decoupling the two guests'
   cmdline needs.
2. At minimum, add a one-line comment at the `boot(nanokernel::timer_guest_elf())`
   call (m4_transparency.rs:351) noting that the leading `'3'` is *not* a
   mode char so the guest runs its default open-window path — so a future
   cmdline edit gets a visible warning.

Option 1 is preferable: it removes the accidental coupling entirely and
restores the timer guest to the exact boot regime its other tests use,
which is also more faithful to "the same guest under the same inputs."
