# Critical and Important

## CRITICAL-1 — "see CI for the cross-cc env" points at something that does not exist

**File:** `docs/ops/test-partitioning.md`, host-runnable table, aarch64 row:

> `cargo clippy --workspace --all-targets --target aarch64-unknown-linux-gnu -- -D warnings`
> | KVM modules are `cfg(target_arch = "x86_64")`-gated (see CI for the cross-cc env if not on arm)

**Problem.** I grepped the entire repo and CI for any cross-compile env
(`CARGO_TARGET_AARCH64_*`, `aarch64-linux-gnu-gcc`, a `.cargo/config.toml`
linker stanza, `CC_aarch64`, "cross-cc"): **zero hits.** There is no
`.cargo/` directory. CI does not cross-compile at all — `.github/workflows/ci.yaml`
runs the arm lane *natively* on `runs-on: ubuntu-24.04-arm` (lines 38, 40).

So the doc's escape hatch ("see CI for the cross-cc env if not on arm") resolves
to nothing. The exact audience b0h names — "any machine incl. macOS/aarch64, how
an agent runs each locally" — is the off-arm dev, and that dev is the one this
pointer fails. An x86_64 host running the documented command needs an aarch64
linker (and typically `CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER` +
the `rustup target add`), none of which is documented here or discoverable in CI.

**Why CRITICAL not Important.** The whole point of a "what runs where" matrix is
that an agent can act on it without a human. This row sends the off-arm agent to
a non-existent reference; the command as written will fail on a stock x86 host.
A docs-table that promises a runnable command and then can't back it is the
specific failure mode b0h was opened to prevent.

**Fix (pick one):**
- *Honest + minimal:* change the note to "aarch64 is built/clipped natively in
  CI on `ubuntu-24.04-arm`; off-arm this needs `rustup target add
  aarch64-unknown-linux-gnu` plus an aarch64 linker (e.g.
  `CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc`)."
- *Or* drop the "see CI" clause entirely and just state the rustup-target
  prerequisite, since CI genuinely has nothing to point at.

---

## IMPORTANT-1 — README mislabels the TSC ioctl-latency numbers as "alignment error"

**File:** `README.md` Measured-numbers section:

> **TSC restore**: KVM_VCPU_TSC_OFFSET device attr chosen over MSR-write
> restore — **932 ns vs 1107 ns worst-case alignment error**

**Problem.** The source of those numbers, `docs/decisions/tsc-alignment.md`,
labels them explicitly as **ns/call** ioctl cost (the table header is
"ns/call, release (N=10,000)"; 932 = `KVM_SET_DEVICE_ATTR(TSC_OFFSET)`,
1,107 = `KVM_SET_MSRS{IA32_TSC}`), and notes "the gap is the ioctl itself, not
the `Msrs` allocation." These are **per-call latencies**, not alignment error
in nanoseconds.

The MSR path's actual *alignment* hazard is the opposite kind of thing: KVM's
TSC-sync heuristics can quantize a write onto an existing sync generation — a
silent *value* perturbation — which is a determinism hazard regardless of call
cost. So the README both (a) renames a latency as an "error" and (b) implies
the decision hinged on a 175 ns accuracy gap, when the decision doc says it
hinged on the sync-heuristic hazard + the per-entry cost (≈3.3 ms/guest-second
at the §10 envelope) + the offset attr being set once and round-tripping
bit-exactly.

**Why IMPORTANT.** Numbers in a README get quoted downstream. "932 ns alignment
error" is a wrong technical claim that contradicts the very doc the line cites
in the same sentence — an internal inconsistency across the doc set, which is
exactly the cross-doc consistency angle. Easy to get right.

**Fix:** "932 ns vs 1107 ns per restore call (ns/call); MSR-write restore also
risks KVM sync-heuristic value quantization — see `docs/decisions/tsc-alignment.md`."
Drop "worst-case alignment error."
