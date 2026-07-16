- Add a future service error/status mapping helper before CreateVm wiring.
  Status: accepted as follow-up scope for rfv/8kb service wiring.

- Tighten docs/proto language around 32-byte BLAKE3 digests and immutable, regular, non-symlink cache files installed atomically.
  Status: done in proto/hypervisor.proto, API.md, and ARCHITECTURE.md.

- `CreateVm` still being unimplemented is acceptable for this bead.
  Status: confirmed; p8g is resolver seam work before real service wiring.

