# Positive notes

- **Build-time 1,000-instruction guarantee is the right design.** The `I`
  macro + `%assign ICOUNT` + `%if ICOUNT != 1000 / %error` makes "exactly
  1,000" un-bypassable: you cannot accidentally ship a 999- or 1,001-
  instruction guest. The one runtime loop is explicitly accounted
  (`%assign ICOUNT ICOUNT + 2 * LOOP_ITERS`) rather than hand-waved. I
  reconstructed the accounting independently: 18 `I`-emitted + 8 loop-
  accounted + 974 `add` filler = 1,000; static emitted = 994; dynamic = 1,000.
  Airtight.

- **The static disassembly matches the macro claims with no surprises.** The
  prompt asked whether nasm could split/fuse anything. It didn't: `mov rcx,
  REP_BYTES` emitted as a single `mov $0x100,%ecx` (one instruction, no
  movzx), `mov dword [rbx+SERIAL_THR], 'M'` as a single `movl $0x4d,…`, the
  REP MOVSB as one `rep movsb`. No instruction the assembler could legally
  split appears in the region.

- **The boundary engine is robust to exactly the spec contradiction this
  iteration found.** `crates/dh-vmm/src/boundary.rs:118-180` (`land_at`) reads
  the counter as the sole progress signal and explicitly comments "never
  assume +1." Whether an exiting instruction retires 0 or 1 cannot break
  landing, because nothing predicts a delta — it polls `counter.read()`. The
  `timer_to_injection` ceil rule (runctl.rs:124-137) maps a vns deadline to a
  target icount; it likewise predicts no instruction's retirement. So the
  997-vs-1000 empiric is confined to documentation, with zero latent runtime
  risk. This is the key reason the verdict is APPROVE.

- **Determinism held under genuinely adversarial perturbation.** 997 was
  bit-identical across 20 in-process boots, 12 fresh processes, all 6 cores
  via taskset, full CPU contention, and `nice -n 19`. The per-thread
  `exclude_host=1` counter design means host scheduling/migration is simply
  invisible to the count — exactly the property a deterministic hypervisor
  needs, and the test demonstrates it rather than asserting it.

- **The smoke asserts serial == [S, M, E] exactly**, which catches stray
  pre-marker MMIO/PIO exits (there are none: the probe showed `s=6`, i.e. only
  the 6 crt0+prologue instructions before the S-OUT, no device traffic).

- **The doc comment on `COUNTING_DELTA_AT_OUT_EXITS` is unusually good.** It
  states the empiric, why it contradicts the vendored spec, that determinism
  is unaffected, the `1000 − 3` arithmetic, and the S-OUT-contributes-0
  reasoning. It is the correct place for this knowledge to live.

- **Honest scoping.** The module docs and the test name both make clear this
  is a *smoke*, not the M2 `counting_semantics` acceptance (bead `gfb`), and
  the reconciliation bead `0sc` was filed rather than silently editing the
  vendored spec. Good discipline.
