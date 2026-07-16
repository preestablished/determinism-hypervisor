# Action items

### Critical

None. Merge is not blocked.

### Important

None.

### Suggestions

All optional; none block merge. Candidates for follow-up beads if pursued.

- **[S1] Harden the phdr-table offset against `usize` overflow.** In
  `crates/dh-vmm/src/boot.rs:106`, replace `let at = phoff + i * phentsize;` with
  `checked_mul`/`checked_add` returning `bad("phdr table overflow")`, matching the
  `checked_add` already used for `p_offset + p_filesz`. Untrusted-input loader code; in
  release the current add/mul wraps silently (memory-safe but parses the wrong offset).

- **[S2] Comment the MMIO-hole PTE's 2 MiB extent.** In `write_page_tables`, note that the
  single 2 MiB PTE deliberately maps `0xD000_0000..0xD020_0000` — a superset of the
  `MMIO_HOLE_LEN = 0x7000` device window — and that the "no memslot ⇒ KVM_EXIT_MMIO"
  contract (not a second memslot as in ARCHITECTURE §2.2) is what makes this safe. Point
  the reader at §2.2.

- **[S3] Optionally validate `e_entry` falls within a loaded PT_LOAD.** Turns an opaque
  triple fault on a bad entry into a clear loader error. Low priority for trusted
  nanokernel images.

- **[S4] De-mirror the page-table unit-test assertions.** Pin the hole PTE to independent
  literals (slot `128`, PTE GPA `0x6400` for `MMIO_HOLE_BASE = 0xD000_0000`) and the
  highest-RAM-page slot to a literal, rather than recomputing them with the same formula
  the implementation uses, so a formula bug cannot pass both sides. (Constants confirmed
  correct during this review.)
