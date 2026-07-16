# Suggestions

### S-1. Preimage has no outer length framing between blob / device_sections / pages — recommend prefixes while v1 is young

The link hashes `H_i || vcpu_blob || device_sections || (le64(idx)||4096)* || le64(icount) || le64(vns)`
with **no length prefix on `vcpu_blob` or on the `device_sections` region as a whole**. I worked the
ambiguity question through:

- **vcpu_blob is constant-length for a fixed machine config.** Every component is serialized at a
  fixed width: 18×u64 (REGS), 8 segments × 21 bytes + tables + control regs + interrupt_bitmap
  (SREGS), the fixed FPU layout, the fixed VCPU_EVENTS field list, DEBUGREGS, then exactly
  `14 + 1` MSR (index,data) pairs of 12 bytes each. There are no length-varying fields. So a trusted
  parser that knows the machine config can locate the blob/device boundary by fixed offset. Good.
- **device_sections is internally framed** (`id u16 || version u16 || len u32 || bytes`), so the
  device region's end is recoverable too — *provided you trust every `len`*.
- **The residual ambiguity is structural, not exploitable by trusted producers.** Because the page
  region's framing `le64(idx) || 4096 bytes` is *the same byte grammar* a malicious device could
  embed in its own section bytes, a forged section could in principle alias what looks like page data.
  All producers here are trusted self-code (`canonical_vcpu_blob`, `DetDevice::snapshot`, the memory
  walk), so this is **not a realistic attack** — but it is a latent footgun: a future device whose
  section happens to be variable-length and whose `len` is computed wrong could shift every subsequent
  boundary and produce a *silently* wrong-but-stable hash (two genuinely different states colliding,
  or one state hashing two ways across versions).

Recommendation: while the format string is still `dh-statehash-v1`, prefix the variable regions —
`h.update(&(vcpu_blob.len() as u64).to_le_bytes())` before the blob, and
`h.update(&(device_sections.len() as u64).to_le_bytes())` before the device region (and/or a page
count before the page loop). This makes the preimage self-delimiting and immune to a future framing
bug. It is a one-time format bump now vs. a `v2` migration later. (Judged realistically: low severity
today, hence a suggestion, but the cost of fixing is near-zero and the cost of *not* is a versioned
format break.)

### S-2. MSR hash order deviates from the §8.1 documented order (IA32_TSC vs SPEC_CTRL position)

§8.1 (ARCHITECTURE.md:643-646) lists the MSRs as: `... PAT, TSC_AUX, IA32_TSC (normalized), SPEC_CTRL`
— i.e. **IA32_TSC then SPEC_CTRL**. The code (`MSR_CAPTURE_LIST`, hash.rs:43-58) emits
`... PAT, TSC_AUX, SPEC_CTRL`, then appends the normalized IA32_TSC slot **last** (hash.rs:300-301).
So the on-wire order is `... TSC_AUX, SPEC_CTRL, IA32_TSC`, the reverse of the doc's tail pair.

This is internally self-consistent and harmless for run-twice-compare (both runs use the same code).
But the bead and §8.1 are the normative source, and M4's DHSNAP codec will serialize the same MSRs;
if M4 follows the doc literally and the state hash follows the code, the two diverge. Recommend either
reordering the code to match §8.1 (IA32_TSC before SPEC_CTRL) or adding a one-line doc note in
ARCHITECTURE §8.1 that the hash places the normalized TSC slot last by construction (since it's not a
GET, it's synthesized). Pick one so code and doc cannot drift.

### S-3. VCPU_EVENTS omits `exception_payload` / `exception_has_payload` — note the assumption explicitly

`canonical_vcpu_blob` hashes `events.exception.{injected,nr,has_error_code,pending,error_code}` but not
the `exception_payload` / `exception_has_payload` fields (present in `kvm_vcpu_events` on kernels that
set `KVM_VCPUEVENT_VALID_PAYLOAD` in `events.flags`). `events.flags` **is** hashed (hash.rs:262), so a
payload-validity *flag* difference perturbs the hash — but two states with the same flags and a
differing `exception_payload` value would hash equal (a missed-state collision).

Phase-1 severity: **low**, by design — snapshots/pauses land at instruction boundaries with nothing in
flight (§8.1: "agenda MUST be empty", boundary engine pauses at instruction boundaries), so no
exception payload should be pending at a hash point. But that "nothing pending at the boundary" is a
*property the boundary engine must guarantee*, and the boundary engine isn't fully landed yet. Since
`events.flags` is already hashed, the cheap belt-and-suspenders fix is to also hash
`exception.payload` (one `to_le_bytes`) so the blob is complete-by-construction rather than
complete-by-assumption. At minimum, add a code comment stating the boundary-quiescence assumption that
makes the omission sound.

### S-4. Page-walk performance for a full 3.25 GiB guest — fine for M3, worth a comment

`push_final_link` walks every page (`0..mem_bytes/4096`), `read_slice` of 4096 into one reused stack
buffer, blake3-updating each. For the max slot (`MMIO_HOLE_BASE`, ~3.25 GiB ⇒ ~832K pages) at blake3's
~GB/s single-thread throughput this is a few seconds — acceptable for the M3 run-twice gate but worth a
`// Phase 1: full serial walk; M4 swaps in the dirty-ring delta + rayon fan-out (§8.2)` note so the
next reader doesn't mistake it for the intended steady-state cost. (§8.2 already specifies rayon-
parallel hashing for the snapshot path; the hash module's serial walk is the Phase-1 stand-in.)

### S-5. `n != MSR_CAPTURE_LIST.len()` error is mapped to `KvmError::Open` — minor variant mismatch

When `get_msrs` returns a short count, the code returns `KvmError::Open(...)` (hash.rs:288-292). `Open`
reads as "failed to open a KVM object", which is misleading for a partial-MSR-read on an already-open
vCPU. Same nit for the closure `kvm_err` mapping every GET failure to `KvmError::Open`. Cosmetic, but a
`KvmError::Capture`/`KvmError::Msr` variant (or `KvmError::Memory`-style "vcpu capture") would make
failures self-describing. Not blocking.
