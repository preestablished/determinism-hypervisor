# Critical & Important findings

**Critical: none. Important: none.**

The two hazards I was tasked to break are documented below as *resolved by experiment*, because the analysis is the load-bearing artifact for this engine and a future reviewer should not have to re-derive it.

---

## (resolved) The write-spanning step cannot overshoot a post-write target

**The hazard (prompt angle 1):** With the fix, a near-step that hits the MMIO-write
exit re-arms TF and re-enters; the `#DB` then fires *after the successor* instruction —
so one "step" spans write + successor. Could a target land exactly between the write and
its successor and get skipped (overshoot)?

**Why it cannot, by construction:**

Trace of the counting guest (`tests/nanokernel/asm/counting.asm`, verified against the
built `counting.elf` disassembly):

```
0x100042  mov  $0xd0000000,%ebx   -> retires, count = 12
0x100047  mov  0x8(%rbx),%rax     -> MMIO READ exit, retires 0  (count stays 12)
0x10004b  movl $0x4d,0x6008(%rbx) -> MMIO WRITE exit, retires 0 (count stays 12)
0x100055  xor  %rax,%rax          -> retires, count = 13
```

The write retires **zero**. Therefore the icount *after* the write equals the icount
*before* it (12), and that value 12 is **already observable at the loop top at
rip 0x100047** (the read), long before the engine ever single-steps the write.
`land_at` breaks at the **first** `c == target` observation. So:

- For `target == 12`: the engine lands at rip 0x100047 (the read) and **never steps the
  write at all** — the spanning step is never reached.
- For `target == 13`: the engine steps the read (MMIO exit, re-arm, trap at the write),
  steps the write (MMIO exit, re-arm, trap at the *successor* `xor`), at which point
  count is exactly 13 and it lands at rip 0x100058 (the `jmp` after `xor`). The spanned
  successor is precisely the instruction that makes count == target, so it lands
  **exactly**, not past.

There is no `target` whose only count==target boundary sits strictly between the write
and its successor, because that count (12) is the *same* count that exists at an earlier
RIP. The overshoot edge is unreachable. **Confirmed live:** `land_at(13)` over repeated
cold boots always returns rip 0x100058, never an Overshoot.

---

## (resolved) Plateau-target landings are deterministic on this class

**The hazard (prompt angle 1):** icount 12 is a *plateau* — it holds across three
(icount, RIP) tuples (0x100047 read, 0x10004b write, and momentarily the pre-`xor`
state). §3.2 declares boundaries as `(icount, RIP)` tuples but does not explicitly say
which RIP `land_at` returns on a plateau, nor that it is replay-stable. If two runs could
stop at different RIPs for target 12 (e.g. PMI skid variance in the far approach changing
where stepping begins), that would be a real determinism finding.

**Result — it is fully deterministic.** Live experiment, `land_at(12)` with margins 8/8:

- 6 sequential cold boots: all rip = 0x100047.
- **240 cold-boot landings under parallel load (4-wide): all rip = 0x100047, 1 distinct.**

Why determinism holds despite the far approach arming a PMI here: the engine only ever
reads the counter *at an instruction boundary* (loop top, after each handled exit / step),
and the guest instruction stream is itself deterministic. The PMI skid only governs how
deep a free-run goes before the engine regains control — it can never carry the count
*past* target (that is the loud Overshoot), and the *first* `c == target` observation is
always at the same (icount, RIP) because the preceding instruction that produced
count == target is fixed. Skid variance does not move the landing RIP; it only moves the
amount of stepping done to reach it.

**Caveat I want on record:** this determinism is *empirical, per-determinism-class*, the
same status §3.1 already assigns to the zero-retirement rule. A plateau target is
arguably an M6-scheduler "don't do that" (targeting an icount with no unique RIP), but the
engine's behavior on one is now measured and stable, which is the right outcome. See the
suggestion in 02 to write this into §3.2.
