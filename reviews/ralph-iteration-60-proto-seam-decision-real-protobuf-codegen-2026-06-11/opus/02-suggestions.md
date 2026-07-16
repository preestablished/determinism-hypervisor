# Suggestions (non-blocking)

### S-1: `sample_lease()` is a public API leftover from the placeholder era

- **File:** `crates/dh-proto/src/lib.rs:27-32`
- The `pub fn sample_lease()` predates this change (it constructed the old
  placeholder `Lease`) and now constructs the generated `v1::Lease`. It is only
  used by the in-crate test. Consider either (a) moving it behind `#[cfg(test)]`
  / into the `tests` module, or (b) keeping it `pub` deliberately as a fixture for
  downstream crates and saying so in a doc comment. Right now it is public with no
  rustdoc, which reads as accidental surface. Low stakes — but bead bcb is a good
  moment to decide whether it stays public.

### S-2: proto inline comments duplicate API.md prose; consider a single pointer

- **File:** `proto/hypervisor.proto:29-47`
- The trailing comments (`// 32 bytes, BLAKE3 of manifest`, `// ARCH §7.4; embedded
  in manifests too`, etc.) are copied verbatim from API.md. That is fine and even
  helpful for codegen readers, but it creates two copies of the same prose that can
  drift. The file header already says "Message/field shapes below are normative
  API.md §2 text," which is the right framing. No action required; just be aware
  that when bcb edits API.md, these inline comments are a second place to keep in
  sync. The byte-for-byte fidelity here is a feature, so I would *not* strip them.

### S-3: decision doc could cross-link the snapstore-client file paths it cites

- **File:** `docs/decisions/proto-seam.md:32-37`
- The "Sibling precedent" bullet describes snapstore-client's mechanism in prose
  but does not give the concrete paths
  (`../snapshot-store/crates/snapstore-client/build.rs` and `src/lib.rs`) that a
  future reader would want to diff against. Adding them would make the "mirrors the
  precedent" claim checkable in one click. The tsc-alignment.md house format is
  prose-with-paths-in-backticks, so this would match the established style.

### S-4: `tsc-alignment.md` includes a "Measured" evidence table; proto-seam.md has no analogous "considered alternatives rejected" delta — but that's appropriate

- **File:** `docs/decisions/proto-seam.md` (whole)
- Comparing against the house format (`docs/decisions/tsc-alignment.md`):
  proto-seam.md follows the same header block (`**Bead:** ... · **Status:**
  decided <date> · **Owner mechanism:** ...`), the same `## Context` / `## Decision`
  / `## Consequences` skeleton, and adds two well-scoped sections (`## The
  re-export seam (kept open)`, `## Skeleton scope vs full surface`). tsc-alignment
  has a `## Measured` table because it was a perf decision; proto-seam is an
  organizational/architecture decision with no measurement, so omitting that
  section is correct, not a gap. **The doc follows the house format.** This is
  recorded as a suggestion only to note one possible addition: a one-line
  "Status of the placeholder we superseded" pointer (the doc covers this in
  Consequences L71-73, so even this is arguably already present). No change needed.

### S-5: consider a `reserved` block comment for bcb in the proto

- **File:** `proto/hypervisor.proto:25` (inside `service HypervisorWorker`)
- Not required, but since the full method set is known (API.md §2 lists all rpcs),
  a comment enumerating the pending rpc names next to the single `GetWorkerInfo`
  would make bcb a pure fill-in and prevent anyone from accidentally renumbering
  the message fields that bcb will add. The file header already points to API.md §2,
  so this is belt-and-suspenders. (Note: proto3 `reserved` applies to field numbers
  within messages, not rpc names in services, so this is a comment, not a literal
  `reserved` statement.)
